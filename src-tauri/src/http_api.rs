// HTTP client for the Maslow / FluidNC network API.
// Endpoints mirror those used by the embedded web UI (httpCmdBuilders.js):
//   GET /command?plain=<cmd>   -> run a gcode / $ command, body = firmware output
//   GET /files?action=list...  -> local filesystem listing
//   GET/POST /upload           -> SD card file operations
// In Phase 0 we only need a connectivity test.

use serde::Serialize;
use std::time::Duration;

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

/// Upload a local file to the machine's SD card via POST /upload (multipart),
/// matching the ESP3D form: `path`, `<fullpath>S` = size, `myfile[]` = file.
#[tauri::command]
pub async fn upload_file(host: String, dir: String, local_path: String) -> Result<String, String> {
    let base = normalize_host(&host);
    let bytes = std::fs::read(&local_path).map_err(|e| format!("read {local_path}: {e}"))?;
    let size = bytes.len();
    let name = base_name(&local_path);
    let dir = if dir.is_empty() { "/".to_string() } else { dir };
    let full = join_dir(&dir, &name);

    let part = reqwest::multipart::Part::bytes(bytes).file_name(full.clone());
    let form = reqwest::multipart::Form::new()
        .text("path", dir.clone())
        .text(format!("{full}S"), size.to_string())
        .part("myfile[]", part);

    let client = http_client(Duration::from_secs(120))?;
    let url = format!("{base}/upload");
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
