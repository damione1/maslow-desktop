// HTTP CalibrationService adapter, mirroring `grpc/calibration.rs`. Pure
// compute, no connection state: calls the free `calibration::solve_calibration`
// function directly rather than a `MaslowService` method. State type is kept
// as `Arc<MaslowService>` only so this router merges with the others; no
// handler here actually reads it.

use crate::calibration;
use crate::http::error::ApiError;
use crate::maslow;
use crate::proto::maslow::v1 as pb;
use crate::service::machine::MaslowService;
use axum::routing::post;
use axum::{Json, Router};
use std::sync::Arc;

pub fn router() -> Router<Arc<MaslowService>> {
    Router::new().route("/v1/calibrations:solve", post(solve_calibration))
}

async fn solve_calibration(
    Json(req): Json<pb::SolveCalibrationRequest>,
) -> Result<Json<pb::SolveResult>, ApiError> {
    let measurements: Vec<calibration::Measurement> = req.measurements.into_iter().map(Into::into).collect();
    let initial: Option<maslow::Anchors> = req.initial.map(Into::into);
    let exclude: Vec<usize> = req.exclude.into_iter().map(|i| i as usize).collect();
    let solver = if req.solver.is_empty() { None } else { Some(req.solver) };

    let result = calibration::solve_calibration(measurements, initial, exclude, solver)
        .map_err(ApiError::invalid_argument)?;
    Ok(Json(result.into()))
}
