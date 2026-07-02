// gRPC FilesService implementation.
//
// ListFiles/DeleteFile/UploadFile/ParseSdToolpath need the connected
// machine's host; `MaslowService` now remembers it (set on `connect()`), so
// these read it from there rather than requiring the caller to repeat it.
//
// LoadLocalToolpath needs no host (it reads a local file).

use crate::proto::maslow::v1 as pb;
use crate::proto::maslow::v1::files_service_server::FilesService;
use crate::service::machine::MaslowService;
use crate::toolpath;
use std::sync::Arc;
use tonic::{Request, Response, Status};

pub struct FilesServiceImpl {
    pub svc: Arc<MaslowService>,
}

fn path_from_resource_name(name: &str) -> &str {
    name.strip_prefix("files/").unwrap_or(name)
}

impl FilesServiceImpl {
    async fn host(&self) -> Result<String, Status> {
        self.svc
            .conn
            .connected_host
            .lock()
            .await
            .clone()
            .ok_or_else(|| Status::failed_precondition("not connected to a machine"))
    }
}

/// The SD listing endpoint's JSON shape (mirrors FileBrowser.svelte's own
/// parsing): `{ files: [{ name, size, isdir? }], ... }`. `size` is `-1` (as
/// either a number or a string, the firmware isn't consistent) for a
/// directory when `isdir` isn't present.
fn parse_file_list(json: &serde_json::Value) -> Vec<pb::File> {
    let Some(entries) = json.get("files").and_then(|f| f.as_array()) else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|e| {
            let name = e.get("name")?.as_str()?.to_string();
            let size_raw = e.get("size");
            let size_bytes = size_raw.and_then(|s| s.as_u64()).or_else(|| {
                size_raw
                    .and_then(|s| s.as_str())
                    .and_then(|s| s.parse::<u64>().ok())
            });
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

#[tonic::async_trait]
impl FilesService for FilesServiceImpl {
    async fn list_files(
        &self,
        request: Request<pb::ListFilesRequest>,
    ) -> Result<Response<pb::ListFilesResponse>, Status> {
        let host = self.host().await?;
        let parent = request.into_inner().parent;
        let json = self.svc.list_files(host, parent).await.map_err(Status::internal)?;
        Ok(Response::new(pb::ListFilesResponse {
            files: parse_file_list(&json),
            // No pagination support: nothing in this app needs it yet.
            next_page_token: String::new(),
        }))
    }

    async fn delete_file(
        &self,
        request: Request<pb::DeleteFileRequest>,
    ) -> Result<Response<pb::DeleteFileResponse>, Status> {
        let host = self.host().await?;
        let name = request.into_inner().name;
        let path = path_from_resource_name(&name);
        let (dir, filename) = match path.rsplit_once('/') {
            Some((d, f)) => (if d.is_empty() { "/".to_string() } else { d.to_string() }, f.to_string()),
            None => ("/".to_string(), path.to_string()),
        };
        self.svc
            .delete_file(host, dir, filename)
            .await
            .map_err(Status::internal)?;
        Ok(Response::new(pb::DeleteFileResponse {}))
    }

    async fn upload_file(
        &self,
        request: Request<pb::UploadFileRequest>,
    ) -> Result<Response<pb::UploadFileResponse>, Status> {
        let host = self.host().await?;
        let r = request.into_inner();
        let name = self
            .svc
            .upload_file(host, r.dir, r.local_path)
            .await
            .map_err(Status::internal)?;
        Ok(Response::new(pb::UploadFileResponse { name }))
    }

    async fn parse_sd_toolpath(
        &self,
        request: Request<pb::ParseSdToolpathRequest>,
    ) -> Result<Response<pb::Toolpath>, Status> {
        let host = self.host().await?;
        let name = request.into_inner().name;
        let path = path_from_resource_name(&name).to_string();
        let tp = self.svc.sd_toolpath(host, path).await.map_err(Status::internal)?;
        Ok(Response::new(tp.into()))
    }

    async fn load_local_toolpath(
        &self,
        request: Request<pb::LoadLocalToolpathRequest>,
    ) -> Result<Response<pb::Toolpath>, Status> {
        let path = request.into_inner().local_path;
        let tp = toolpath::load_toolpath(path).await.map_err(Status::internal)?;
        Ok(Response::new(tp.into()))
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
