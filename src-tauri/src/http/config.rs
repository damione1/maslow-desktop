// HTTP ConfigService adapter, mirroring `grpc/config.rs`. `forceRefresh`
// triggers a fresh `$CD` dump and polls the snapshot cache for it to arrive
// (the dump is captured asynchronously over the WS, not returned directly by
// `request_config_dump`).

use crate::http::error::ApiError;
use crate::proto::maslow::v1 as pb;
use crate::service::machine::MaslowService;
use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use std::sync::Arc;
use std::time::Duration;

pub fn router() -> Router<Arc<MaslowService>> {
    Router::new()
        .route("/v1/configEntries", get(list_config_entries))
        .route("/v1/configEntries/*path", get(get_config_entry).patch(update_config_entry))
}

const CONFIG_DUMP_POLL_INTERVAL: Duration = Duration::from_millis(100);
const CONFIG_DUMP_TIMEOUT: Duration = Duration::from_secs(3);

/// Request a fresh `$CD` dump and poll the snapshot cache until it lands or
/// `CONFIG_DUMP_TIMEOUT` elapses. Matches `ConfigServiceImpl::refreshed_entries`
/// in the gRPC layer.
async fn refreshed_entries(svc: &MaslowService) -> Result<Vec<crate::fluidnc::ConfigEntry>, ApiError> {
    svc.request_config_dump().await.map_err(ApiError::internal)?;
    let deadline = tokio::time::Instant::now() + CONFIG_DUMP_TIMEOUT;
    loop {
        {
            let snap = svc.snapshot.read().unwrap();
            if let Some(entries) = snap.config_entries.clone() {
                return Ok(entries);
            }
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(ApiError::deadline_exceeded("timed out waiting for a fresh config dump"));
        }
        tokio::time::sleep(CONFIG_DUMP_POLL_INTERVAL).await;
    }
}

#[derive(Deserialize, Default)]
struct ListConfigQuery {
    #[serde(default, rename = "forceRefresh")]
    force_refresh: bool,
}

async fn list_config_entries(
    State(svc): State<Arc<MaslowService>>,
    Query(q): Query<ListConfigQuery>,
) -> Result<Json<pb::ListConfigEntriesResponse>, ApiError> {
    let entries = if q.force_refresh {
        refreshed_entries(&svc).await?
    } else {
        let snap = svc.snapshot.read().unwrap();
        snap.config_entries.clone().unwrap_or_default()
    };
    // No pagination support: nothing in this app needs it yet. page_size/
    // page_token accepted by the proto message are simply not read here.
    Ok(Json(pb::ListConfigEntriesResponse {
        config_entries: entries.into_iter().map(Into::into).collect(),
        next_page_token: String::new(),
    }))
}

async fn get_config_entry(
    State(svc): State<Arc<MaslowService>>,
    Path(path): Path<String>,
) -> Result<Json<pb::ConfigEntry>, ApiError> {
    let entry = {
        let snap = svc.snapshot.read().unwrap();
        snap.config_entries
            .as_ref()
            .and_then(|entries| entries.iter().find(|e| e.path == path))
            .cloned()
    };
    entry
        .map(|e| Json(e.into()))
        .ok_or_else(|| ApiError::not_found(format!("no config entry \"{path}\" (has a config dump run yet?)")))
}

#[derive(Deserialize)]
struct UpdateConfigBody {
    value: String,
    #[serde(default)]
    save: bool,
}

async fn update_config_entry(
    State(svc): State<Arc<MaslowService>>,
    Path(path): Path<String>,
    Json(body): Json<UpdateConfigBody>,
) -> Result<Json<pb::ConfigEntry>, ApiError> {
    svc.write_setting(path.clone(), body.value.clone())
        .await
        .map_err(ApiError::internal)?;
    if body.save {
        svc.save_config().await.map_err(ApiError::internal)?;
    }
    Ok(Json(pb::ConfigEntry {
        path,
        value: body.value,
        kind: pb::ConfigKind::Unspecified as i32,
    }))
}
