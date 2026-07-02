// HTTP JobService adapter, mirroring `grpc/job.rs`. "jobs/current" is a
// singleton: GetJob and the mutating actions all report the latest cached
// job progress, or an "idle" Job when nothing has ever streamed.

use crate::grpc::stream;
use crate::http::error::ApiError;
use crate::http::sse::json_event_stream;
use crate::proto::maslow::v1 as pb;
use crate::service::machine::MaslowService;
use axum::extract::State;
use axum::response::sse::{Event, Sse};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::Stream;
use std::convert::Infallible;
use std::sync::Arc;

pub fn router() -> Router<Arc<MaslowService>> {
    Router::new()
        .route("/v1/jobs/current", get(get_job))
        .route("/v1/jobs/current:saved", get(get_saved_job))
        .route("/v1/jobs/current:start", post(start_job))
        .route("/v1/jobs/current:pause", post(pause_job))
        .route("/v1/jobs/current:resume", post(resume_job))
        .route("/v1/jobs/current:stop", post(stop_job))
        .route("/v1/jobs/current:watch", get(watch_job_progress))
}

/// Latest known job state, or an idle placeholder when no job has ever run on
/// this connection. Matches `JobServiceImpl::current_job` in the gRPC layer.
fn current_job(svc: &MaslowService) -> pb::Job {
    let snap = svc.snapshot.read().unwrap();
    snap.job_progress.clone().map(Into::into).unwrap_or_else(|| pb::Job {
        state: "idle".to_string(),
        ..Default::default()
    })
}

async fn get_job(State(svc): State<Arc<MaslowService>>) -> Json<pb::Job> {
    Json(current_job(&svc))
}

async fn get_saved_job(State(svc): State<Arc<MaslowService>>) -> Json<pb::GetSavedJobResponse> {
    let saved_job = svc.stream_saved().map(Into::into);
    Json(pb::GetSavedJobResponse { saved_job })
}

async fn start_job(
    State(svc): State<Arc<MaslowService>>,
    Json(req): Json<pb::StartJobRequest>,
) -> Result<Json<pb::StartJobResponse>, ApiError> {
    let total = svc
        .start_job(req.path, req.start_index as usize)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(pb::StartJobResponse { total: total as u64 }))
}

async fn pause_job(State(svc): State<Arc<MaslowService>>) -> Result<Json<pb::Job>, ApiError> {
    svc.pause_job().await.map_err(ApiError::internal)?;
    Ok(Json(current_job(&svc)))
}

async fn resume_job(State(svc): State<Arc<MaslowService>>) -> Result<Json<pb::Job>, ApiError> {
    svc.resume_job().await.map_err(ApiError::internal)?;
    Ok(Json(current_job(&svc)))
}

async fn stop_job(State(svc): State<Arc<MaslowService>>) -> Result<Json<pb::Job>, ApiError> {
    svc.stop_job().await.map_err(ApiError::internal)?;
    Ok(Json(current_job(&svc)))
}

/// SSE equivalent of the gRPC `WatchJobProgress` streaming RPC, built on the
/// same `grpc::stream::job_progress_stream` adapter so the job-progress
/// filtering logic is not duplicated per transport.
async fn watch_job_progress(State(svc): State<Arc<MaslowService>>) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = svc.events.subscribe();
    json_event_stream(stream::job_progress_stream(rx))
}
