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
use std::future::Future;
use std::sync::Arc;
use tonic::{Request, Status};

/// Build a per-service auth interceptor. Reads `svc.api_settings` live on
/// every call (not a value captured once at server-spawn time), so a key
/// regenerated or an enable/disable toggle flipped while the server is
/// running takes effect on the very next request. Expects an `authorization`
/// metadata entry of the form `Bearer <key>`.
// `Result<Request<()>, Status>` is mandated by tonic's `Interceptor` trait
// (`FnMut(Request<()>) -> Result<Request<()>, Status>`); the closure's error
// variant size is not something a caller of this crate's own API controls.
#[allow(clippy::result_large_err)]
fn make_interceptor(svc: Arc<MaslowService>) -> impl FnMut(Request<()>) -> Result<Request<()>, Status> + Clone {
    move |req: Request<()>| {
        let key = req
            .metadata()
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "));
        let authorized = key.is_some_and(|k| {
            let settings = svc.api_settings.read().unwrap();
            crate::auth::check_key(&settings, k)
        });
        if authorized {
            Ok(req)
        } else {
            Err(Status::unauthenticated("missing or invalid API key"))
        }
    }
}

/// Start the gRPC server on a background Tauri task, listening on
/// `svc.api_settings`'s current `port_grpc` (loopback only). Shuts down
/// gracefully when `shutdown` resolves, which lets a settings change
/// (enable/disable, key regeneration) restart the server on demand rather
/// than only at app exit.
pub fn spawn_server(svc: Arc<MaslowService>, shutdown: impl Future<Output = ()> + Send + 'static) {
    tauri::async_runtime::spawn(async move {
        let port = svc.api_settings.read().unwrap().port_grpc;
        let addr = format!("127.0.0.1:{port}")
            .parse()
            .expect("127.0.0.1:<port> is a valid socket address");
        eprintln!("gRPC server listening on 127.0.0.1:{port}");
        let result = tonic::transport::Server::builder()
            .add_service(MachineServiceServer::with_interceptor(
                machine::MachineServiceImpl { svc: svc.clone() },
                make_interceptor(svc.clone()),
            ))
            .add_service(JobServiceServer::with_interceptor(
                job::JobServiceImpl { svc: svc.clone() },
                make_interceptor(svc.clone()),
            ))
            .add_service(ConfigServiceServer::with_interceptor(
                config::ConfigServiceImpl { svc: svc.clone() },
                make_interceptor(svc.clone()),
            ))
            .add_service(FilesServiceServer::with_interceptor(
                files::FilesServiceImpl { svc: svc.clone() },
                make_interceptor(svc.clone()),
            ))
            .add_service(CalibrationServiceServer::with_interceptor(
                calibration::CalibrationServiceImpl,
                make_interceptor(svc.clone()),
            ))
            .serve_with_shutdown(addr, shutdown)
            .await;
        if let Err(e) = result {
            eprintln!("gRPC server exited with error: {e}");
        }
    });
}
