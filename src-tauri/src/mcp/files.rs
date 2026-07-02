// MCP FilesService tools, mirroring `http/files.rs`.
//
// list_files/delete_file/upload_file/parse_sd_toolpath need the connected
// machine's host, read from `MaslowService::conn.connected_host` (set on
// connect) rather than from the caller. load_local_toolpath needs no host (it
// reads a local file on the machine running this server).

use crate::mcp::{err, ok, ok_json, McpServer};
use crate::proto::maslow::v1 as pb;
use crate::service::machine::MaslowService;
use crate::toolpath;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Deserialize, JsonSchema, Default)]
pub struct ListFilesParams {
    /// SD directory path, e.g. "/" or "/jobs". Defaults to the root.
    #[serde(default)]
    pub parent: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct DeleteFileParams {
    /// SD-absolute path of the file to delete, e.g. "/job.nc" or "/sub/job.nc".
    pub path: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct UploadFileParams {
    /// SD directory to upload into, e.g. "/" or "/jobs".
    #[serde(default)]
    pub dir: String,
    /// Path to the local file to upload, on the machine running this server (not the MCP client).
    pub local_path: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct ParseSdToolpathParams {
    /// SD-absolute path of the G-code file to parse, e.g. "/job.nc".
    pub path: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct LoadLocalToolpathParams {
    /// Path to a local G-code file to parse, on the machine running this server (not the MCP client).
    pub local_path: String,
}

/// The SD listing endpoint's JSON shape (mirrors `http::files::parse_file_list`
/// and FileBrowser.svelte's own parsing): `{ files: [{ name, size, isdir? }],
/// ... }`. `size` is `-1` (as either a number or a string, the firmware isn't
/// consistent) for a directory when `isdir` isn't present.
fn parse_file_list(json: &serde_json::Value) -> Vec<pb::File> {
    let Some(entries) = json.get("files").and_then(|f| f.as_array()) else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|e| {
            let name = e.get("name")?.as_str()?.to_string();
            let size_raw = e.get("size");
            let size_bytes = size_raw
                .and_then(|s| s.as_u64())
                .or_else(|| size_raw.and_then(|s| s.as_str()).and_then(|s| s.parse::<u64>().ok()));
            let is_directory = e.get("isdir").and_then(|v| v.as_bool()).unwrap_or(false)
                || size_raw.map(|s| s.to_string().trim_matches('"') == "-1").unwrap_or(false);
            Some(pb::File {
                name,
                size_bytes: size_bytes.unwrap_or(0),
                is_directory,
            })
        })
        .collect()
}

async fn connected_host(svc: &MaslowService) -> Result<String, String> {
    svc.conn
        .connected_host
        .lock()
        .await
        .clone()
        .ok_or_else(|| "not connected to a machine".to_string())
}

#[tool_router(router = tool_router_files, vis = "pub(crate)")]
impl McpServer {
    #[tool(
        description = "List files and directories on the machine's SD card. Read-only; requires an active connection."
    )]
    async fn list_files(&self, Parameters(req): Parameters<ListFilesParams>) -> CallToolResult {
        let host = match connected_host(&self.svc).await {
            Ok(h) => h,
            Err(e) => return err(e),
        };
        match self.svc.list_files(host, req.parent).await {
            Ok(json) => ok_json(&pb::ListFilesResponse {
                files: parse_file_list(&json),
                next_page_token: String::new(),
            }),
            Err(e) => err(e),
        }
    }

    #[tool(
        description = "Delete a file from the machine's SD card. This permanently removes the file; requires an active connection."
    )]
    async fn delete_file(&self, Parameters(req): Parameters<DeleteFileParams>) -> CallToolResult {
        let host = match connected_host(&self.svc).await {
            Ok(h) => h,
            Err(e) => return err(e),
        };
        let (dir, filename) = match req.path.rsplit_once('/') {
            Some((d, f)) => (if d.is_empty() { "/".to_string() } else { d.to_string() }, f.to_string()),
            None => ("/".to_string(), req.path.clone()),
        };
        match self.svc.delete_file(host, dir, filename).await {
            Ok(_) => ok(),
            Err(e) => err(e),
        }
    }

    #[tool(
        description = "Upload a local G-code file to the machine's SD card. Requires an active connection; local_path is read from the machine running this server, not the MCP client."
    )]
    async fn upload_file(&self, Parameters(req): Parameters<UploadFileParams>) -> CallToolResult {
        let host = match connected_host(&self.svc).await {
            Ok(h) => h,
            Err(e) => return err(e),
        };
        match self.svc.upload_file(host, req.dir, req.local_path).await {
            Ok(name) => ok_json(&pb::UploadFileResponse { name }),
            Err(e) => err(e),
        }
    }

    #[tool(
        description = "Download a G-code file from the machine's SD card and parse it into a 2D toolpath preview, without running it. Read-only; requires an active connection."
    )]
    async fn parse_sd_toolpath(&self, Parameters(req): Parameters<ParseSdToolpathParams>) -> CallToolResult {
        let host = match connected_host(&self.svc).await {
            Ok(h) => h,
            Err(e) => return err(e),
        };
        match self.svc.sd_toolpath(host, req.path).await {
            Ok(tp) => ok_json(&pb::Toolpath::from(tp)),
            Err(e) => err(e),
        }
    }

    #[tool(
        description = "Parse a local G-code file into a 2D toolpath preview, without running it. Read-only; does not require a machine connection. local_path is read from the machine running this server, not the MCP client."
    )]
    async fn load_local_toolpath(&self, Parameters(req): Parameters<LoadLocalToolpathParams>) -> CallToolResult {
        match toolpath::load_toolpath(req.local_path).await {
            Ok(tp) => ok_json(&pb::Toolpath::from(tp)),
            Err(e) => err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_files_and_directories() {
        let json = serde_json::json!({
            "files": [
                {"name": "job.nc", "size": 12345},
                {"name": "sub", "size": -1, "isdir": true},
                {"name": "legacy.nc", "size": "6789"},
            ],
        });
        let files = parse_file_list(&json);
        assert_eq!(files.len(), 3);
        assert_eq!(files[0], pb::File { name: "job.nc".to_string(), size_bytes: 12345, is_directory: false });
        assert_eq!(files[1], pb::File { name: "sub".to_string(), size_bytes: 0, is_directory: true });
        assert_eq!(files[2], pb::File { name: "legacy.nc".to_string(), size_bytes: 6789, is_directory: false });
    }

    #[test]
    fn missing_files_key_is_empty() {
        let json = serde_json::json!({});
        assert!(parse_file_list(&json).is_empty());
    }

    #[test]
    fn list_files_params_defaults_parent_to_empty() {
        let params: ListFilesParams = serde_json::from_value(serde_json::json!({})).unwrap();
        assert_eq!(params.parent, "");
    }

    /// `delete_file`'s dir/filename split, exercised the same way the tool
    /// handler does before calling `MaslowService::delete_file`.
    #[test]
    fn delete_file_path_splits_into_dir_and_filename() {
        let (dir, filename) = match "/sub/job.nc".rsplit_once('/') {
            Some((d, f)) => (if d.is_empty() { "/".to_string() } else { d.to_string() }, f.to_string()),
            None => ("/".to_string(), "/sub/job.nc".to_string()),
        };
        assert_eq!(dir, "/sub");
        assert_eq!(filename, "job.nc");
    }

    #[test]
    fn delete_file_path_at_root_splits_to_root_dir() {
        let (dir, filename) = match "/job.nc".rsplit_once('/') {
            Some((d, f)) => (if d.is_empty() { "/".to_string() } else { d.to_string() }, f.to_string()),
            None => ("/".to_string(), "/job.nc".to_string()),
        };
        assert_eq!(dir, "/");
        assert_eq!(filename, "job.nc");
    }
}
