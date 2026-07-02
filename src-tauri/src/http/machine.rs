// HTTP MachineService adapter. Mirrors `grpc/machine.rs` one for one: every
// handler delegates to the same `MaslowService` method (or, for the ping /
// firmware-version endpoints, the same `http_api` free function) so the
// command strings live in exactly one place regardless of transport.

use crate::http::error::ApiError;
use crate::http_api;
use crate::proto::maslow::v1 as pb;
use crate::service::machine::MaslowService;
use axum::extract::{Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use std::sync::Arc;

pub fn router() -> Router<Arc<MaslowService>> {
    Router::new()
        .route("/v1/machines/default", get(get_machine_status))
        .route("/v1/machines/default/actionPolicy", get(get_action_policy))
        .route("/v1/machines/default:snapshot", get(get_snapshot))
        .route("/v1/machines/default:connect", post(connect))
        .route("/v1/machines/default:disconnect", post(disconnect))
        .route("/v1/machines/default:jog", post(jog))
        .route("/v1/machines/default:home", post(home))
        .route("/v1/machines/default:unlock", post(unlock))
        .route("/v1/machines/default:hold", post(hold))
        .route("/v1/machines/default:resume", post(resume))
        .route("/v1/machines/default:retract", post(retract))
        .route("/v1/machines/default:extend", post(extend))
        .route("/v1/machines/default:takeSlack", post(take_slack))
        .route("/v1/machines/default:comply", post(comply))
        .route("/v1/machines/default:calibrate", post(calibrate))
        .route("/v1/machines/default:stop", post(stop))
        .route("/v1/machines/default:eStop", post(e_stop))
        .route("/v1/machines/default:zero", post(zero))
        .route("/v1/machines/default:sendLine", post(send_line))
        .route("/v1/machines/default:sendRealtime", post(send_realtime))
        .route("/v1/machines/default:writeSetting", post(write_setting))
        .route("/v1/machines/default:saveConfig", post(save_config))
        .route("/v1/machines/default:pingMachine", get(ping_machine))
        .route("/v1/machines/default:firmwareVersion", get(get_firmware_version))
}

async fn get_machine_status(State(svc): State<Arc<MaslowService>>) -> Result<Json<pb::MachineStatus>, ApiError> {
    let status = {
        let snap = svc.snapshot.read().unwrap();
        snap.status.clone()
    };
    status
        .map(|s| Json(s.into()))
        .ok_or_else(|| ApiError::not_found("no status observed yet - is the machine connected?"))
}

async fn get_action_policy(State(svc): State<Arc<MaslowService>>) -> Result<Json<pb::ActionPolicy>, ApiError> {
    let policy = {
        let snap = svc.snapshot.read().unwrap();
        snap.action_policy.clone()
    };
    policy
        .map(|p| Json(p.into()))
        .ok_or_else(|| ApiError::not_found("no status observed yet - is the machine connected?"))
}

async fn get_snapshot(State(svc): State<Arc<MaslowService>>) -> Json<pb::Snapshot> {
    let snap = svc.snapshot.read().unwrap();
    Json(crate::grpc::convert::snapshot_to_proto(&snap))
}

async fn connect(
    State(svc): State<Arc<MaslowService>>,
    Json(req): Json<pb::ConnectRequest>,
) -> Result<Json<pb::ConnectResponse>, ApiError> {
    svc.connect(req.host).await.map_err(ApiError::internal)?;
    Ok(Json(pb::ConnectResponse {}))
}

async fn disconnect(State(svc): State<Arc<MaslowService>>) -> Result<Json<pb::DisconnectResponse>, ApiError> {
    svc.disconnect().await.map_err(ApiError::internal)?;
    Ok(Json(pb::DisconnectResponse {}))
}

async fn jog(
    State(svc): State<Arc<MaslowService>>,
    Json(req): Json<pb::JogRequest>,
) -> Result<Json<pb::JogResponse>, ApiError> {
    svc.jog(req.dx, req.dy, req.dz, req.feed).await.map_err(ApiError::internal)?;
    Ok(Json(pb::JogResponse {}))
}

async fn home(State(svc): State<Arc<MaslowService>>) -> Result<Json<pb::HomeResponse>, ApiError> {
    svc.home().await.map_err(ApiError::internal)?;
    Ok(Json(pb::HomeResponse {}))
}

async fn unlock(State(svc): State<Arc<MaslowService>>) -> Result<Json<pb::UnlockResponse>, ApiError> {
    svc.unlock().await.map_err(ApiError::internal)?;
    Ok(Json(pb::UnlockResponse {}))
}

async fn hold(State(svc): State<Arc<MaslowService>>) -> Result<Json<pb::HoldResponse>, ApiError> {
    svc.hold().await.map_err(ApiError::internal)?;
    Ok(Json(pb::HoldResponse {}))
}

async fn resume(State(svc): State<Arc<MaslowService>>) -> Result<Json<pb::ResumeResponse>, ApiError> {
    svc.resume().await.map_err(ApiError::internal)?;
    Ok(Json(pb::ResumeResponse {}))
}

async fn zero(
    State(svc): State<Arc<MaslowService>>,
    Json(req): Json<pb::ZeroRequest>,
) -> Result<Json<pb::ZeroResponse>, ApiError> {
    svc.zero(req.axes).await.map_err(ApiError::internal)?;
    Ok(Json(pb::ZeroResponse {}))
}

async fn retract(State(svc): State<Arc<MaslowService>>) -> Result<Json<pb::RetractResponse>, ApiError> {
    svc.retract().await.map_err(ApiError::internal)?;
    Ok(Json(pb::RetractResponse {}))
}

async fn extend(State(svc): State<Arc<MaslowService>>) -> Result<Json<pb::ExtendResponse>, ApiError> {
    svc.extend().await.map_err(ApiError::internal)?;
    Ok(Json(pb::ExtendResponse {}))
}

async fn take_slack(State(svc): State<Arc<MaslowService>>) -> Result<Json<pb::TakeSlackResponse>, ApiError> {
    svc.take_slack().await.map_err(ApiError::internal)?;
    Ok(Json(pb::TakeSlackResponse {}))
}

async fn comply(State(svc): State<Arc<MaslowService>>) -> Result<Json<pb::ComplyResponse>, ApiError> {
    svc.comply().await.map_err(ApiError::internal)?;
    Ok(Json(pb::ComplyResponse {}))
}

async fn calibrate(State(svc): State<Arc<MaslowService>>) -> Result<Json<pb::CalibrateResponse>, ApiError> {
    svc.calibrate().await.map_err(ApiError::internal)?;
    Ok(Json(pb::CalibrateResponse {}))
}

async fn stop(State(svc): State<Arc<MaslowService>>) -> Result<Json<pb::StopResponse>, ApiError> {
    svc.stop().await.map_err(ApiError::internal)?;
    Ok(Json(pb::StopResponse {}))
}

async fn e_stop(State(svc): State<Arc<MaslowService>>) -> Result<Json<pb::EStopResponse>, ApiError> {
    svc.estop().await.map_err(ApiError::internal)?;
    Ok(Json(pb::EStopResponse {}))
}

async fn send_line(
    State(svc): State<Arc<MaslowService>>,
    Json(req): Json<pb::SendLineRequest>,
) -> Result<Json<pb::SendLineResponse>, ApiError> {
    svc.send_line(req.line).await.map_err(ApiError::internal)?;
    Ok(Json(pb::SendLineResponse {}))
}

async fn send_realtime(
    State(svc): State<Arc<MaslowService>>,
    Json(req): Json<pb::SendRealtimeRequest>,
) -> Result<Json<pb::SendRealtimeResponse>, ApiError> {
    let byte: u8 = req
        .byte
        .try_into()
        .map_err(|_| ApiError::invalid_argument("byte must fit in a single byte (0-255)"))?;
    svc.send_realtime(byte).await.map_err(ApiError::internal)?;
    Ok(Json(pb::SendRealtimeResponse {}))
}

async fn write_setting(
    State(svc): State<Arc<MaslowService>>,
    Json(req): Json<pb::WriteSettingRequest>,
) -> Result<Json<pb::WriteSettingResponse>, ApiError> {
    svc.write_setting(req.path, req.value).await.map_err(ApiError::internal)?;
    Ok(Json(pb::WriteSettingResponse {}))
}

async fn save_config(State(svc): State<Arc<MaslowService>>) -> Result<Json<pb::SaveConfigResponse>, ApiError> {
    svc.save_config().await.map_err(ApiError::internal)?;
    Ok(Json(pb::SaveConfigResponse {}))
}

#[derive(Deserialize)]
struct HostQuery {
    host: String,
}

async fn ping_machine(Query(q): Query<HostQuery>) -> Json<pb::PingMachineResponse> {
    let result = http_api::ping_machine(q.host).await;
    Json(pb::PingMachineResponse {
        reachable: result.reachable,
        status: u32::from(result.status),
        info: result.info,
    })
}

async fn get_firmware_version(Query(q): Query<HostQuery>) -> Json<pb::GetFirmwareVersionResponse> {
    let version = http_api::firmware_version(q.host).await;
    Json(pb::GetFirmwareVersionResponse { version })
}
