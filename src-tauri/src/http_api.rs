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
