// HTTP client for the Maslow / FluidNC network API.
// Endpoints mirror those used by the embedded web UI (httpCmdBuilders.js):
//   GET /command?plain=<cmd>   -> run a gcode / $ command, body = firmware output
//   GET /files?action=list...  -> local filesystem listing
//   GET/POST /upload           -> SD card file operations
// In Phase 0 we only need a connectivity test.

use crate::service::machine::MaslowService;
use serde::Serialize;
use std::sync::Arc;
use std::time::Duration;
use tauri::State;

#[derive(Serialize)]
pub struct PingResult {
    pub reachable: bool,
    pub status: u16,
    /// Firmware info from [ESP420] (system stats) when reachable, else error message.
    pub info: String,
}

pub(crate) fn normalize_host(host: &str) -> String {
    let h = host.trim().trim_end_matches('/');
    if h.starts_with("http://") || h.starts_with("https://") {
        h.to_string()
    } else {
        format!("http://{}", h)
    }
}

#[tauri::command]
pub async fn upload_file(
    state: State<'_, Arc<MaslowService>>,
    host: String,
    dir: String,
    local_path: String,
) -> Result<String, String> {
    state.upload_file(host, dir, local_path).await
}

#[tauri::command]
pub async fn list_files(
    state: State<'_, Arc<MaslowService>>,
    host: String,
    path: String,
) -> Result<serde_json::Value, String> {
    state.list_files(host, path).await
}

#[tauri::command]
pub async fn delete_file(
    state: State<'_, Arc<MaslowService>>,
    host: String,
    dir: String,
    filename: String,
) -> Result<serde_json::Value, String> {
    state.delete_file(host, dir, filename).await
}

#[tauri::command]
pub async fn sd_toolpath(
    state: State<'_, Arc<MaslowService>>,
    host: String,
    path: String,
) -> Result<crate::toolpath::Toolpath, String> {
    state.sd_toolpath(host, path).await
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
}
