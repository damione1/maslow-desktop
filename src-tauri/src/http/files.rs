// HTTP FilesService adapter, mirroring `grpc/files.rs`.
//
// ListFiles/DeleteFile/UploadFile/ParseSdToolpath need the connected
// machine's host, read from `MaslowService::conn.connected_host` (set on
// `connect()`) rather than from the caller.
//
// `ParseSdToolpath` cannot use a "/v1/files/{*path}:parseToolpath" route:
// axum (matchit) only allows a `*wildcard` segment as the last element of a
// route, so a literal ":parseToolpath" suffix after it does not compile.
// Following the spec's documented fallback, the path is instead carried in
// the JSON body (`name`, matching `pb::ParseSdToolpathRequest`), the same
// style already used by `:start`/`:solve`-type actions elsewhere.
//
// LoadLocalToolpath needs no host (it reads a local file).

use crate::http::error::ApiError;
use crate::proto::maslow::v1 as pb;
use crate::service::machine::MaslowService;
use crate::toolpath;
use axum::extract::{Path, Query, State};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::Deserialize;
use std::sync::Arc;

pub fn router() -> Router<Arc<MaslowService>> {
    Router::new()
        .route("/v1/files", get(list_files))
        .route("/v1/files/*path", delete(delete_file))
        .route("/v1/files:upload", post(upload_file))
        .route("/v1/files:parseToolpath", post(parse_sd_toolpath))
        .route("/v1/files:loadLocalToolpath", post(load_local_toolpath))
}

async fn connected_host(svc: &MaslowService) -> Result<String, ApiError> {
    svc.conn
        .connected_host
        .lock()
        .await
        .clone()
        .ok_or_else(|| ApiError::failed_precondition("not connected to a machine"))
}

/// The SD listing endpoint's JSON shape (mirrors `grpc::files::parse_file_list`
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

#[derive(Deserialize, Default)]
struct ListFilesQuery {
    #[serde(default)]
    parent: String,
}

async fn list_files(
    State(svc): State<Arc<MaslowService>>,
    Query(q): Query<ListFilesQuery>,
) -> Result<Json<pb::ListFilesResponse>, ApiError> {
    let host = connected_host(&svc).await?;
    let json = svc.list_files(host, q.parent).await.map_err(ApiError::internal)?;
    Ok(Json(pb::ListFilesResponse {
        files: parse_file_list(&json),
        next_page_token: String::new(),
    }))
}

async fn delete_file(
    State(svc): State<Arc<MaslowService>>,
    Path(path): Path<String>,
) -> Result<Json<pb::DeleteFileResponse>, ApiError> {
    let host = connected_host(&svc).await?;
    let (dir, filename) = match path.rsplit_once('/') {
        Some((d, f)) => (if d.is_empty() { "/".to_string() } else { d.to_string() }, f.to_string()),
        None => ("/".to_string(), path.clone()),
    };
    svc.delete_file(host, dir, filename).await.map_err(ApiError::internal)?;
    Ok(Json(pb::DeleteFileResponse {}))
}

async fn upload_file(
    State(svc): State<Arc<MaslowService>>,
    Json(req): Json<pb::UploadFileRequest>,
) -> Result<Json<pb::UploadFileResponse>, ApiError> {
    let host = connected_host(&svc).await?;
    let name = svc
        .upload_file(host, req.dir, req.local_path)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(pb::UploadFileResponse { name }))
}

fn path_from_resource_name(name: &str) -> &str {
    name.strip_prefix("files/").unwrap_or(name)
}

async fn parse_sd_toolpath(
    State(svc): State<Arc<MaslowService>>,
    Json(req): Json<pb::ParseSdToolpathRequest>,
) -> Result<Json<pb::Toolpath>, ApiError> {
    let host = connected_host(&svc).await?;
    let path = path_from_resource_name(&req.name).to_string();
    let tp = svc.sd_toolpath(host, path).await.map_err(ApiError::internal)?;
    Ok(Json(tp.into()))
}

async fn load_local_toolpath(
    Json(req): Json<pb::LoadLocalToolpathRequest>,
) -> Result<Json<pb::Toolpath>, ApiError> {
    let tp = toolpath::load_toolpath(req.local_path).await.map_err(ApiError::internal)?;
    Ok(Json(tp.into()))
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
            "total": "7.40 GB",
            "used": "1.20 GB",
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
}
