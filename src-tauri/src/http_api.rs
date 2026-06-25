// HTTP client for the Maslow / FluidNC network API.
// Endpoints mirror those used by the embedded web UI (httpCmdBuilders.js):
//   GET /command?plain=<cmd>   -> run a gcode / $ command, body = firmware output
//   GET /files?action=list...  -> local filesystem listing
//   GET/POST /upload           -> SD card file operations
// In Phase 0 we only need a connectivity test.

use crate::connection::ConnState;
use serde::Serialize;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tauri::State;

#[derive(Serialize)]
pub struct PingResult {
    pub reachable: bool,
    pub status: u16,
    /// Firmware info from [ESP420] (system stats) when reachable, else error message.
    pub info: String,
}

fn normalize_host(host: &str) -> String {
    let h = host.trim().trim_end_matches('/');
    if h.starts_with("http://") || h.starts_with("https://") {
        h.to_string()
    } else {
        format!("http://{}", h)
    }
}

fn http_client(timeout: Duration) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(timeout)
        .cookie_store(true)
        .build()
        .map_err(|e| e.to_string())
}

fn base_name(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or(path).to_string()
}

fn join_dir(dir: &str, name: &str) -> String {
    let d = if dir.is_empty() { "/" } else { dir };
    if d.ends_with('/') {
        format!("{d}{name}")
    } else {
        format!("{d}/{name}")
    }
}

/// Parse a firmware `formatBytes` string (e.g. `"7.40 GB"`, `"512 B"`) back into
/// bytes. Approximate (the firmware rounds to 2 decimals) but fine for a guard.
fn parse_bytes(s: &str) -> Option<u64> {
    let s = s.trim();
    let (num, unit) = s.split_once(' ')?;
    let v: f64 = num.trim().parse().ok()?;
    let mult: f64 = match unit.trim().to_ascii_uppercase().as_str() {
        "B" => 1.0,
        "KB" => 1024.0,
        "MB" => 1024f64.powi(2),
        "GB" => 1024f64.powi(3),
        "TB" => 1024f64.powi(4),
        _ => return None,
    };
    Some((v * mult) as u64)
}

/// Human-readable byte size for error messages.
fn human_bytes(b: u64) -> String {
    const U: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{b} B")
    } else {
        format!("{v:.2} {}", U[i])
    }
}

/// Best-effort free space on the SD card (bytes) via the listing endpoint, which
/// reports `total`/`used` as formatted strings. Returns None if unavailable or
/// unparseable, in which case the caller skips the pre-flight check.
async fn sd_free_bytes(client: &reqwest::Client, base: &str, dir: &str) -> Option<u64> {
    let url = format!("{base}/upload");
    let resp = client
        .get(&url)
        .query(&[("path", dir), ("action", "list")])
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let v: serde_json::Value = resp.json().await.ok()?;
    let total = parse_bytes(v.get("total")?.as_str()?)?;
    let used = parse_bytes(v.get("used")?.as_str()?)?;
    Some(total.saturating_sub(used))
}

/// Upload a local file to the machine's SD card via POST /upload (multipart),
/// matching the ESP3D form: `path`, `<fullpath>S` = size, `myfile[]` = file.
#[tauri::command]
pub async fn upload_file(
    state: State<'_, ConnState>,
    host: String,
    dir: String,
    local_path: String,
) -> Result<String, String> {
    let base = normalize_host(&host);
    let bytes = std::fs::read(&local_path).map_err(|e| format!("read {local_path}: {e}"))?;
    let size = bytes.len();
    let name = base_name(&local_path);
    let dir = if dir.is_empty() { "/".to_string() } else { dir };
    let full = join_dir(&dir, &name);

    let client = http_client(Duration::from_secs(120))?;

    // Pre-flight free-space check: fail fast with a clear message rather than
    // half-filling the card. The firmware also rejects an over-size upload at
    // uploadStart, so this is only a friendly early guard (and it's skipped if
    // the listing can't be parsed).
    if let Some(free) = sd_free_bytes(&client, &base, &dir).await {
        if size as u64 > free {
            return Err(format!(
                "Not enough space on the SD card: {} needed, {} free.",
                human_bytes(size as u64),
                human_bytes(free)
            ));
        }
    }

    // Match the embedded UI's form exactly: `path` (dir), `<fullpath>S` (size,
    // sent before the file so it's available at UPLOAD_FILE_START), then the
    // file part named `myfile[]` whose filename is the full path.
    let part = reqwest::multipart::Part::bytes(bytes).file_name(full.clone());
    let form = reqwest::multipart::Form::new()
        .text("path", dir.clone())
        .text(format!("{full}S"), size.to_string())
        .part("myfile[]", part);

    let url = format!("{base}/upload");

    // Silence the WebSocket polling for the duration: the firmware's SD/flash
    // write stalls both cores, and concurrent traffic during that window can
    // corrupt the write (the embedded UI disables its ping the same way). The
    // guard restores polling even if the upload errors out.
    let upload_active = state.upload_active.clone();
    upload_active.store(true, Ordering::Relaxed);
    let result = async {
        let resp = client
            .post(&url)
            .multipart(form)
            .send()
            .await
            .map_err(|e| format!("upload: {e}"))?;
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        if status.is_success() {
            Ok(body)
        } else {
            Err(format!("upload failed ({status}): {body}"))
        }
    }
    .await;
    upload_active.store(false, Ordering::Relaxed);
    result
}

/// List files in an SD directory: GET /upload?path=<p>&action=list.
/// Returns the raw JSON the firmware sends ({ files: [...], path, ... }).
#[tauri::command]
pub async fn list_files(host: String, path: String) -> Result<serde_json::Value, String> {
    let base = normalize_host(&host);
    let p = if path.is_empty() { "/".to_string() } else { path };
    let client = http_client(Duration::from_secs(10))?;
    let url = format!("{base}/upload");
    let resp = client
        .get(&url)
        .query(&[("path", p.as_str()), ("action", "list")])
        .send()
        .await
        .map_err(|e| format!("list: {e}"))?;
    resp.json::<serde_json::Value>()
        .await
        .map_err(|e| format!("list parse: {e}"))
}

/// Delete a file on the SD card: GET /upload?path=<p>&action=delete&filename=<n>.
#[tauri::command]
pub async fn delete_file(host: String, dir: String, filename: String) -> Result<serde_json::Value, String> {
    let base = normalize_host(&host);
    let d = if dir.is_empty() { "/".to_string() } else { dir };
    let client = http_client(Duration::from_secs(10))?;
    let url = format!("{base}/upload");
    let resp = client
        .get(&url)
        .query(&[
            ("path", d.as_str()),
            ("action", "delete"),
            ("filename", filename.as_str()),
        ])
        .send()
        .await
        .map_err(|e| format!("delete: {e}"))?;
    resp.json::<serde_json::Value>()
        .await
        .map_err(|e| format!("delete parse: {e}"))
}

/// Read the frame anchor configuration from the firmware
/// (`$/kinematics/MaslowKinematics/`) over HTTP and parse it. Used to tell
/// whether the machine already has valid anchors loaded (calibrated) so the UI
/// can offer the short "apply tension" resume path instead of full calibration.
#[tauri::command]
pub async fn read_maslow_anchors(host: String) -> Result<crate::maslow::Anchors, String> {
    let base = normalize_host(&host);
    let client = http_client(Duration::from_secs(10))?;
    let url = format!("{base}/command");
    let resp = client
        .get(&url)
        .query(&[("plain", "$/kinematics/MaslowKinematics/")])
        .send()
        .await
        .map_err(|e| format!("read anchors: {e}"))?;
    let body = resp.text().await.map_err(|e| format!("read anchors: {e}"))?;
    crate::maslow::parse_anchors(&body).ok_or_else(|| format!("no anchor config in response: {body}"))
}

/// Root-level Maslow config keys (the `M` prefix is "Maslow"), confirmed in
/// `MachineConfig::groupM4Items()` AND verified valid on the real machine
/// (FluidNC v1.21). The anchor coordinates live separately under the
/// `kinematics/MaslowKinematics/` section and are read in one shot.
///
/// `Maslow_Apply_Tension_Belt_Retraction_Limit` / `_Allow_Limiting` are
/// deliberately NOT here: they only exist from firmware v1.22.0 onward and the
/// v1.21 machine rejects them with `error:3`. The per-key reader below also
/// skips any key the firmware reports as invalid, so this list staying ahead of
/// (or behind) a given firmware never breaks the load.
const MASLOW_ROOT_KEYS: &[&str] = &[
    "Maslow_Work_Area_X",
    "Maslow_Work_Area_Y",
    "Maslow_Work_Area_Center_Offset_X",
    "Maslow_Work_Area_Center_Offset_Y",
    "Maslow_Retract_Current_Threshold",
    "Maslow_Extend_Dist",
];

/// True when a `/command` body is the firmware's rejection of an unknown
/// setting key. The firmware answers an invalid `$/<key>` read with HTTP 200
/// but a body like `[MSG:ERR: Invalid setting or command: /<key>]` + `error:N`,
/// so we must inspect the body (not the HTTP status) to detect it.
fn is_rejected(body: &str) -> bool {
    let b = body.to_ascii_lowercase();
    b.contains("invalid setting") || b.contains("error:")
}

/// Run a `$`/gcode command over the synchronous HTTP `/command` endpoint and
/// return the firmware's textual output (the same channel output the WebSocket
/// would carry). Errors on a non-2xx HTTP status.
async fn run_command(host: &str, plain: &str) -> Result<String, String> {
    let base = normalize_host(host);
    let client = http_client(Duration::from_secs(10))?;
    let url = format!("{base}/command");
    let resp = client
        .get(&url)
        .query(&[("plain", plain)])
        .send()
        .await
        .map_err(|e| format!("{plain}: {e}"))?;
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if status.is_success() {
        Ok(body)
    } else {
        Err(format!("{plain}: HTTP {status}: {body}"))
    }
}

/// Read the full Maslow-relevant configuration (anchors + work area + tension)
/// over HTTP and parse it into a typed struct. The kinematics anchors come from
/// a single section dump; the root `Maslow_*` items are read individually and
/// independently — each key is best effort. A key the firmware does not know is
/// rejected with `error:3` (HTTP 200, error in the body); we skip such replies
/// instead of folding their error text into the dump, so one unknown key never
/// aborts the whole load (graceful degradation across firmware versions).
#[tauri::command]
pub async fn read_maslow_config(host: String) -> Result<crate::maslow::MaslowConfig, String> {
    let mut dump = run_command(&host, "$/kinematics/MaslowKinematics/").await?;
    for key in MASLOW_ROOT_KEYS {
        match run_command(&host, &format!("$/{key}")).await {
            // Only fold in a successful echo; drop firmware rejections and
            // transport errors so they cannot poison the parse.
            Ok(body) if !is_rejected(&body) => {
                dump.push('\n');
                dump.push_str(&body);
            }
            _ => {}
        }
    }
    crate::maslow::parse_maslow_config(&dump)
        .ok_or_else(|| format!("no Maslow config in response: {dump}"))
}

/// Write a single FluidNC setting: `$/<path>=<value>`. `path` is the full config
/// path (e.g. `Maslow_Work_Area_X` or `kinematics/MaslowKinematics/tlX`). The
/// firmware silently accepts a float/int write; a rejected write echoes an
/// `error:`/`[MSG:ERR...]`, which we surface as an Err so the UI can react.
#[tauri::command]
pub async fn write_maslow_setting(host: String, path: String, value: String) -> Result<String, String> {
    let body = run_command(&host, &format!("$/{path}={value}")).await?;
    if body.to_ascii_lowercase().contains("error") {
        return Err(body.trim().to_string());
    }
    Ok(body.trim().to_string())
}

/// Persist the current (runtime-edited) config to flash via `$CO`
/// (Config/Overwrite). Without this, edited settings are lost on reboot.
#[tauri::command]
pub async fn save_maslow_config(host: String) -> Result<String, String> {
    let body = run_command(&host, "$CO").await?;
    if body.to_ascii_lowercase().contains("error") {
        return Err(body.trim().to_string());
    }
    Ok(body.trim().to_string())
}

/// Test reachability of the machine and fetch a bit of firmware info.
#[tauri::command]
pub async fn ping_machine(host: String) -> PingResult {
    let base = normalize_host(&host);
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .cookie_store(true)
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return PingResult { reachable: false, status: 0, info: format!("client error: {e}") }
        }
    };

    // [ESP420] returns FluidNC system stats; URL-encoded brackets.
    let url = format!("{base}/command?plain=%5BESP420%5D");
    match client.get(&url).send().await {
        Ok(resp) => {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            PingResult { reachable: status == 200, status, info: body }
        }
        Err(e) => PingResult { reachable: false, status: 0, info: format!("unreachable: {e}") },
    }
}

/// Firmware version the embedded UI shows, parsed from the `[ESP800]`
/// `FW version: FluidNC <ver>` line (e.g. "1.22"). Returns None if unreachable
/// or unparseable so the topbar can simply hide it.
#[tauri::command]
pub async fn firmware_version(host: String) -> Option<String> {
    let base = normalize_host(&host);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .cookie_store(true)
        .build()
        .ok()?;
    let url = format!("{base}/command?plain=%5BESP800%5D");
    let body = client.get(&url).send().await.ok()?.text().await.ok()?;
    parse_fw_version(&body)
}

/// Pull the version token out of a `FW version: FluidNC <ver> # …` line. Drops
/// the leading "FluidNC" label and keeps the first whitespace/`#`-delimited
/// token (a git-describe string like `1.22` or `1.22-3-gabc`).
fn parse_fw_version(body: &str) -> Option<String> {
    for line in body.lines() {
        if let Some((_, after)) = line.split_once("FW version:") {
            let after = after.trim();
            let after = after.strip_prefix("FluidNC").map(str::trim).unwrap_or(after);
            let ver = after
                .split([' ', '#', '\t'])
                .find(|t| !t.is_empty())
                .unwrap_or("");
            if !ver.is_empty() {
                return Some(ver.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_firmware_version() {
        assert_eq!(
            parse_fw_version("FW version: FluidNC 1.22 # FW target:grbl-embedded"),
            Some("1.22".to_string())
        );
        assert_eq!(
            parse_fw_version("[MSG]\nFW version: FluidNC 1.22-3-gabc  # FW HW:Direct SD"),
            Some("1.22-3-gabc".to_string())
        );
        assert_eq!(parse_fw_version("no version here"), None);
    }

    #[test]
    fn parses_format_bytes() {
        assert_eq!(parse_bytes("512 B"), Some(512));
        assert_eq!(parse_bytes("1.00 KB"), Some(1024));
        assert_eq!(parse_bytes("2.00 MB"), Some(2 * 1024 * 1024));
        assert_eq!(parse_bytes("7.40 GB"), Some((7.40 * 1024f64.powi(3)) as u64));
        assert_eq!(parse_bytes("garbage"), None);
        assert_eq!(parse_bytes("1.0 PB"), None);
    }

    #[test]
    fn formats_human_bytes() {
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(1024), "1.00 KB");
        assert_eq!(human_bytes(1024 * 1024), "1.00 MB");
    }
}
