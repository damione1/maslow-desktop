// SD card file operations against the Maslow / FluidNC HTTP API: upload,
// listing, deletion, and downloading a job for toolpath preview. Endpoints
// mirror those used by the embedded web UI (httpCmdBuilders.js). Tauri
// commands in `http_api.rs` are one-line delegates to these methods.

use crate::http_api::normalize_host;
use crate::service::machine::MaslowService;
use std::sync::atomic::Ordering;
use std::time::Duration;

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

impl MaslowService {
    /// Upload a local file to the machine's SD card via POST /upload (multipart),
    /// matching the ESP3D form: `path`, `<fullpath>S` = size, `myfile[]` = file.
    pub async fn upload_file(
        &self,
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
        let upload_active = self.conn.upload_active.clone();
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
    pub async fn list_files(&self, host: String, path: String) -> Result<serde_json::Value, String> {
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
    pub async fn delete_file(
        &self,
        host: String,
        dir: String,
        filename: String,
    ) -> Result<serde_json::Value, String> {
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

    /// Download a G-code file from the SD card and parse it into a 2D toolpath for
    /// preview, without running it. FluidNC serves SD files directly at
    /// `/SD/<path>` (the same href the embedded UI uses for downloads). `path` is
    /// the absolute SD path, e.g. `/job.nc` or `/sub/job.nc`.
    pub async fn sd_toolpath(&self, host: String, path: String) -> Result<crate::toolpath::Toolpath, String> {
        let base = normalize_host(&host);
        let p = if path.starts_with('/') {
            path
        } else {
            format!("/{path}")
        };
        let client = http_client(Duration::from_secs(20))?;
        let url = format!("{base}/SD{p}");
        let resp = client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("download: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!("download: HTTP {}", resp.status().as_u16()));
        }
        let body = resp.text().await.map_err(|e| format!("download read: {e}"))?;
        tauri::async_runtime::spawn_blocking(move || {
            let lines: Vec<String> = body.lines().filter_map(crate::streaming::clean_line).collect();
            crate::toolpath::parse_toolpath(&lines)
        })
        .await
        .map_err(|e| format!("sd_toolpath join: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
