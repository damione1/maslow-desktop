// gRPC transport adapter: one file per service, each a thin wrapper that
// delegates to `MaslowService` (or, for calibration, the pure-compute
// `calibration` module). No business logic lives in this module.

pub mod calibration;
pub mod config;
pub mod convert;
pub mod files;
pub mod job;
pub mod machine;

use crate::proto::maslow::v1::calibration_service_server::CalibrationServiceServer;
use crate::proto::maslow::v1::config_service_server::ConfigServiceServer;
use crate::proto::maslow::v1::files_service_server::FilesServiceServer;
use crate::proto::maslow::v1::job_service_server::JobServiceServer;
use crate::proto::maslow::v1::machine_service_server::MachineServiceServer;
use crate::service::machine::MaslowService;
use std::sync::Arc;

/// Loopback-only, hardcoded port. There is no authentication yet, so the
/// server must not be reachable from the network; revisit once auth and an
/// enable/disable toggle land.
const GRPC_ADDR: &str = "127.0.0.1:50051";

/// Start the gRPC server on a background Tauri task. Runs for the life of the
/// app; there is no shutdown handle yet, so the OS reclaims the socket on exit.
pub fn spawn_server(svc: Arc<MaslowService>) {
    tauri::async_runtime::spawn(async move {
        let addr = GRPC_ADDR.parse().expect("GRPC_ADDR is a valid socket address");
        eprintln!("gRPC server listening on {GRPC_ADDR}");
        let result = tonic::transport::Server::builder()
            .add_service(MachineServiceServer::new(machine::MachineServiceImpl { svc: svc.clone() }))
            .add_service(JobServiceServer::new(job::JobServiceImpl { svc: svc.clone() }))
            .add_service(ConfigServiceServer::new(config::ConfigServiceImpl { svc: svc.clone() }))
            .add_service(FilesServiceServer::new(files::FilesServiceImpl { svc: svc.clone() }))
            .add_service(CalibrationServiceServer::new(calibration::CalibrationServiceImpl))
            .serve(addr)
            .await;
        if let Err(e) = result {
            eprintln!("gRPC server exited with error: {e}");
        }
    });
}
