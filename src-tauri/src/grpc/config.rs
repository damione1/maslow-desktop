// gRPC ConfigService implementation. `force_refresh` triggers a fresh `$CD`
// dump and polls the snapshot cache for it to arrive (the dump is captured
// asynchronously over the WS, not returned directly by `request_config_dump`).

use crate::proto::maslow::v1 as pb;
use crate::proto::maslow::v1::config_service_server::ConfigService;
use crate::service::machine::MaslowService;
use std::sync::Arc;
use std::time::Duration;
use tonic::{Request, Response, Status};

pub struct ConfigServiceImpl {
    pub svc: Arc<MaslowService>,
}

const CONFIG_DUMP_POLL_INTERVAL: Duration = Duration::from_millis(100);
const CONFIG_DUMP_TIMEOUT: Duration = Duration::from_secs(3);

fn path_from_resource_name(name: &str) -> &str {
    name.strip_prefix("configEntries/").unwrap_or(name)
}

impl ConfigServiceImpl {
    /// Request a fresh `$CD` dump and poll the snapshot cache until it lands
    /// or `CONFIG_DUMP_TIMEOUT` elapses.
    async fn refreshed_entries(&self) -> Result<Vec<crate::fluidnc::ConfigEntry>, Status> {
        self.svc.request_config_dump().await.map_err(Status::internal)?;
        let deadline = tokio::time::Instant::now() + CONFIG_DUMP_TIMEOUT;
        loop {
            {
                let snap = self.svc.snapshot.read().unwrap();
                if let Some(entries) = snap.config_entries.clone() {
                    return Ok(entries);
                }
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(Status::deadline_exceeded("timed out waiting for a fresh config dump"));
            }
            tokio::time::sleep(CONFIG_DUMP_POLL_INTERVAL).await;
        }
    }
}

#[tonic::async_trait]
impl ConfigService for ConfigServiceImpl {
    async fn list_config_entries(
        &self,
        request: Request<pb::ListConfigEntriesRequest>,
    ) -> Result<Response<pb::ListConfigEntriesResponse>, Status> {
        let force_refresh = request.into_inner().force_refresh;
        let entries = if force_refresh {
            self.refreshed_entries().await?
        } else {
            let snap = self.svc.snapshot.read().unwrap();
            snap.config_entries.clone().unwrap_or_default()
        };
        // No pagination support: nothing in this app needs it yet. page_size/
        // page_token are accepted but ignored; every entry comes back at once.
        Ok(Response::new(pb::ListConfigEntriesResponse {
            config_entries: entries.into_iter().map(Into::into).collect(),
            next_page_token: String::new(),
        }))
    }

    async fn get_config_entry(
        &self,
        request: Request<pb::GetConfigEntryRequest>,
    ) -> Result<Response<pb::ConfigEntry>, Status> {
        let name = request.into_inner().name;
        let path = path_from_resource_name(&name).to_string();
        let entry = {
            let snap = self.svc.snapshot.read().unwrap();
            snap.config_entries
                .as_ref()
                .and_then(|entries| entries.iter().find(|e| e.path == path))
                .cloned()
        };
        entry
            .map(|e| Response::new(e.into()))
            .ok_or_else(|| Status::not_found(format!("no config entry \"{path}\" (has a config dump run yet?)")))
    }

    async fn update_config_entry(
        &self,
        request: Request<pb::UpdateConfigEntryRequest>,
    ) -> Result<Response<pb::ConfigEntry>, Status> {
        let r = request.into_inner();
        let entry = r
            .config_entry
            .ok_or_else(|| Status::invalid_argument("config_entry is required"))?;
        self.svc
            .write_setting(entry.path.clone(), entry.value.clone())
            .await
            .map_err(Status::internal)?;
        if r.save {
            self.svc.save_config().await.map_err(Status::internal)?;
        }
        Ok(Response::new(entry))
    }
}
