// MCP ConfigService tools, mirroring `http/config.rs`. `force_refresh`
// triggers a fresh `$CD` dump and polls the snapshot cache for it to arrive
// (the dump is captured asynchronously over the WS, not returned directly by
// `request_config_dump`).

use crate::mcp::{err, ok_json, McpServer};
use crate::proto::maslow::v1 as pb;
use crate::service::machine::MaslowService;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use std::time::Duration;

const CONFIG_DUMP_POLL_INTERVAL: Duration = Duration::from_millis(100);
const CONFIG_DUMP_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Deserialize, JsonSchema, Default)]
pub struct ListConfigEntriesParams {
    /// When true, request a fresh dump from the machine instead of serving the last cached one.
    #[serde(default)]
    pub force_refresh: bool,
}

#[derive(Deserialize, JsonSchema)]
pub struct GetConfigEntryParams {
    /// Full FluidNC config path, e.g. "axes/x/steps_per_mm".
    pub path: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct UpdateConfigEntryParams {
    /// Full FluidNC config path, e.g. "axes/x/steps_per_mm".
    pub path: String,
    /// New value to write, as a string.
    pub value: String,
    /// Whether to also persist the change to flash after writing it.
    #[serde(default)]
    pub save: bool,
}

/// Request a fresh `$CD` dump and poll the snapshot cache until it lands or
/// `CONFIG_DUMP_TIMEOUT` elapses. Matches `http::config::refreshed_entries`.
async fn refreshed_entries(svc: &MaslowService) -> Result<Vec<crate::fluidnc::ConfigEntry>, String> {
    svc.request_config_dump().await?;
    let deadline = tokio::time::Instant::now() + CONFIG_DUMP_TIMEOUT;
    loop {
        {
            let snap = svc.snapshot.read().unwrap();
            if let Some(entries) = snap.config_entries.clone() {
                return Ok(entries);
            }
        }
        if tokio::time::Instant::now() >= deadline {
            return Err("timed out waiting for a fresh config dump".to_string());
        }
        tokio::time::sleep(CONFIG_DUMP_POLL_INTERVAL).await;
    }
}

#[tool_router(router = tool_router_config, vis = "pub(crate)")]
impl McpServer {
    #[tool(
        description = "List every known FluidNC config entry (path, value, kind). Read-only; pass force_refresh to request a fresh dump from the machine instead of serving the cached one."
    )]
    async fn list_config_entries(&self, Parameters(req): Parameters<ListConfigEntriesParams>) -> CallToolResult {
        let entries = if req.force_refresh {
            match refreshed_entries(&self.svc).await {
                Ok(entries) => entries,
                Err(e) => return err(e),
            }
        } else {
            let snap = self.svc.snapshot.read().unwrap();
            snap.config_entries.clone().unwrap_or_default()
        };
        // No pagination support: nothing in this app needs it yet.
        ok_json(&pb::ListConfigEntriesResponse {
            config_entries: entries.into_iter().map(Into::into).collect(),
            next_page_token: String::new(),
        })
    }

    #[tool(
        description = "Get a single FluidNC config entry by its path. Read-only; requires a config dump to have run at least once (see list_config_entries)."
    )]
    async fn get_config_entry(&self, Parameters(req): Parameters<GetConfigEntryParams>) -> CallToolResult {
        let entry = {
            let snap = self.svc.snapshot.read().unwrap();
            snap.config_entries
                .as_ref()
                .and_then(|entries| entries.iter().find(|e| e.path == req.path))
                .cloned()
        };
        match entry {
            Some(e) => ok_json(&pb::ConfigEntry::from(e)),
            None => err(format!("no config entry \"{}\" (has a config dump run yet?)", req.path)),
        }
    }

    #[tool(
        description = "Write a FluidNC config entry's value, optionally persisting it to flash. This changes machine behavior (e.g. steps-per-mm, work area size); an incorrect value can cause unexpected motion on the next command."
    )]
    async fn update_config_entry(&self, Parameters(req): Parameters<UpdateConfigEntryParams>) -> CallToolResult {
        if let Err(e) = self.svc.write_setting(req.path.clone(), req.value.clone()).await {
            return err(e);
        }
        if req.save {
            if let Err(e) = self.svc.save_config().await {
                return err(e);
            }
        }
        ok_json(&pb::ConfigEntry {
            path: req.path,
            value: req.value,
            kind: pb::ConfigKind::Unspecified as i32,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_config_entries_params_defaults_force_refresh_to_false() {
        let params: ListConfigEntriesParams = serde_json::from_value(serde_json::json!({})).unwrap();
        assert!(!params.force_refresh);
    }

    #[test]
    fn update_config_entry_params_defaults_save_to_false() {
        let params: UpdateConfigEntryParams =
            serde_json::from_value(serde_json::json!({"path": "axes/x/steps_per_mm", "value": "100"})).unwrap();
        assert_eq!(params.path, "axes/x/steps_per_mm");
        assert_eq!(params.value, "100");
        assert!(!params.save);
    }
}
