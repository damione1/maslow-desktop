// gRPC CalibrationService implementation. Pure compute, no connection state:
// calls the free `calibration::solve_calibration` function directly rather
// than a `MaslowService` method.

use crate::calibration;
use crate::maslow;
use crate::proto::maslow::v1 as pb;
use crate::proto::maslow::v1::calibration_service_server::CalibrationService;
use tonic::{Request, Response, Status};

pub struct CalibrationServiceImpl;

#[tonic::async_trait]
impl CalibrationService for CalibrationServiceImpl {
    async fn solve_calibration(
        &self,
        request: Request<pb::SolveCalibrationRequest>,
    ) -> Result<Response<pb::SolveResult>, Status> {
        let r = request.into_inner();
        let measurements: Vec<calibration::Measurement> = r.measurements.into_iter().map(Into::into).collect();
        let initial: Option<maslow::Anchors> = r.initial.map(Into::into);
        let exclude: Vec<usize> = r.exclude.into_iter().map(|i| i as usize).collect();
        let solver = if r.solver.is_empty() { None } else { Some(r.solver) };

        let result = calibration::solve_calibration(measurements, initial, exclude, solver)
            .map_err(Status::invalid_argument)?;
        Ok(Response::new(result.into()))
    }
}
