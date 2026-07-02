// gRPC MachineService implementation. Every method delegates to a
// `MaslowService` convenience method (either an existing one, or one of the
// action wrappers added in `service/machine.rs`), so the command strings
// live in exactly one place.

use crate::http_api;
use crate::proto::maslow::v1 as pb;
use crate::proto::maslow::v1::machine_service_server::MachineService;
use crate::service::machine::MaslowService;
use std::pin::Pin;
use std::sync::Arc;
use tonic::{Request, Response, Status};

pub struct MachineServiceImpl {
    pub svc: Arc<MaslowService>,
}

#[tonic::async_trait]
impl MachineService for MachineServiceImpl {
    async fn get_machine_status(
        &self,
        _request: Request<pb::GetMachineStatusRequest>,
    ) -> Result<Response<pb::MachineStatus>, Status> {
        let status = {
            let snap = self.svc.snapshot.read().unwrap();
            snap.status.clone()
        };
        status
            .map(|s| Response::new(s.into()))
            .ok_or_else(|| Status::not_found("no status observed yet - is the machine connected?"))
    }

    async fn get_action_policy(
        &self,
        _request: Request<pb::GetActionPolicyRequest>,
    ) -> Result<Response<pb::ActionPolicy>, Status> {
        let policy = {
            let snap = self.svc.snapshot.read().unwrap();
            snap.action_policy.clone()
        };
        policy
            .map(|p| Response::new(p.into()))
            .ok_or_else(|| Status::not_found("no status observed yet - is the machine connected?"))
    }

    async fn get_snapshot(
        &self,
        _request: Request<pb::GetSnapshotRequest>,
    ) -> Result<Response<pb::Snapshot>, Status> {
        let snap = self.svc.snapshot.read().unwrap();
        Ok(Response::new(super::convert::snapshot_to_proto(&snap)))
    }

    async fn connect(&self, request: Request<pb::ConnectRequest>) -> Result<Response<pb::ConnectResponse>, Status> {
        let host = request.into_inner().host;
        self.svc.connect(host).await.map_err(Status::internal)?;
        Ok(Response::new(pb::ConnectResponse {}))
    }

    async fn disconnect(
        &self,
        _request: Request<pb::DisconnectRequest>,
    ) -> Result<Response<pb::DisconnectResponse>, Status> {
        self.svc.disconnect().await.map_err(Status::internal)?;
        Ok(Response::new(pb::DisconnectResponse {}))
    }

    async fn jog(&self, request: Request<pb::JogRequest>) -> Result<Response<pb::JogResponse>, Status> {
        let r = request.into_inner();
        self.svc.jog(r.dx, r.dy, r.dz, r.feed).await.map_err(Status::internal)?;
        Ok(Response::new(pb::JogResponse {}))
    }

    async fn home(&self, _request: Request<pb::HomeRequest>) -> Result<Response<pb::HomeResponse>, Status> {
        self.svc.home().await.map_err(Status::internal)?;
        Ok(Response::new(pb::HomeResponse {}))
    }

    async fn unlock(&self, _request: Request<pb::UnlockRequest>) -> Result<Response<pb::UnlockResponse>, Status> {
        self.svc.unlock().await.map_err(Status::internal)?;
        Ok(Response::new(pb::UnlockResponse {}))
    }

    async fn hold(&self, _request: Request<pb::HoldRequest>) -> Result<Response<pb::HoldResponse>, Status> {
        self.svc.hold().await.map_err(Status::internal)?;
        Ok(Response::new(pb::HoldResponse {}))
    }

    async fn resume(&self, _request: Request<pb::ResumeRequest>) -> Result<Response<pb::ResumeResponse>, Status> {
        self.svc.resume().await.map_err(Status::internal)?;
        Ok(Response::new(pb::ResumeResponse {}))
    }

    async fn zero(&self, request: Request<pb::ZeroRequest>) -> Result<Response<pb::ZeroResponse>, Status> {
        let axes = request.into_inner().axes;
        self.svc.zero(axes).await.map_err(Status::internal)?;
        Ok(Response::new(pb::ZeroResponse {}))
    }

    async fn retract(&self, _request: Request<pb::RetractRequest>) -> Result<Response<pb::RetractResponse>, Status> {
        self.svc.retract().await.map_err(Status::internal)?;
        Ok(Response::new(pb::RetractResponse {}))
    }

    async fn extend(&self, _request: Request<pb::ExtendRequest>) -> Result<Response<pb::ExtendResponse>, Status> {
        self.svc.extend().await.map_err(Status::internal)?;
        Ok(Response::new(pb::ExtendResponse {}))
    }

    async fn take_slack(
        &self,
        _request: Request<pb::TakeSlackRequest>,
    ) -> Result<Response<pb::TakeSlackResponse>, Status> {
        self.svc.take_slack().await.map_err(Status::internal)?;
        Ok(Response::new(pb::TakeSlackResponse {}))
    }

    async fn comply(&self, _request: Request<pb::ComplyRequest>) -> Result<Response<pb::ComplyResponse>, Status> {
        self.svc.comply().await.map_err(Status::internal)?;
        Ok(Response::new(pb::ComplyResponse {}))
    }

    async fn calibrate(
        &self,
        _request: Request<pb::CalibrateRequest>,
    ) -> Result<Response<pb::CalibrateResponse>, Status> {
        self.svc.calibrate().await.map_err(Status::internal)?;
        Ok(Response::new(pb::CalibrateResponse {}))
    }

    async fn stop(&self, _request: Request<pb::StopRequest>) -> Result<Response<pb::StopResponse>, Status> {
        self.svc.stop().await.map_err(Status::internal)?;
        Ok(Response::new(pb::StopResponse {}))
    }

    async fn e_stop(&self, _request: Request<pb::EStopRequest>) -> Result<Response<pb::EStopResponse>, Status> {
        self.svc.estop().await.map_err(Status::internal)?;
        Ok(Response::new(pb::EStopResponse {}))
    }

    async fn send_line(&self, request: Request<pb::SendLineRequest>) -> Result<Response<pb::SendLineResponse>, Status> {
        let line = request.into_inner().line;
        self.svc.send_line(line).await.map_err(Status::internal)?;
        Ok(Response::new(pb::SendLineResponse {}))
    }

    async fn send_realtime(
        &self,
        request: Request<pb::SendRealtimeRequest>,
    ) -> Result<Response<pb::SendRealtimeResponse>, Status> {
        let byte = request.into_inner().byte;
        let byte: u8 = byte
            .try_into()
            .map_err(|_| Status::invalid_argument("byte must fit in a single byte (0-255)"))?;
        self.svc.send_realtime(byte).await.map_err(Status::internal)?;
        Ok(Response::new(pb::SendRealtimeResponse {}))
    }

    async fn write_setting(
        &self,
        request: Request<pb::WriteSettingRequest>,
    ) -> Result<Response<pb::WriteSettingResponse>, Status> {
        let r = request.into_inner();
        self.svc.write_setting(r.path, r.value).await.map_err(Status::internal)?;
        Ok(Response::new(pb::WriteSettingResponse {}))
    }

    async fn save_config(
        &self,
        _request: Request<pb::SaveConfigRequest>,
    ) -> Result<Response<pb::SaveConfigResponse>, Status> {
        self.svc.save_config().await.map_err(Status::internal)?;
        Ok(Response::new(pb::SaveConfigResponse {}))
    }

    async fn ping_machine(
        &self,
        request: Request<pb::PingMachineRequest>,
    ) -> Result<Response<pb::PingMachineResponse>, Status> {
        let host = request.into_inner().host;
        let result = http_api::ping_machine(host).await;
        Ok(Response::new(pb::PingMachineResponse {
            reachable: result.reachable,
            status: u32::from(result.status),
            info: result.info,
        }))
    }

    async fn get_firmware_version(
        &self,
        request: Request<pb::GetFirmwareVersionRequest>,
    ) -> Result<Response<pb::GetFirmwareVersionResponse>, Status> {
        let host = request.into_inner().host;
        let version = http_api::firmware_version(host).await;
        Ok(Response::new(pb::GetFirmwareVersionResponse { version }))
    }

    type WatchMachineEventsStream =
        Pin<Box<dyn tonic::codegen::tokio_stream::Stream<Item = Result<pb::MachineEvent, Status>> + Send>>;

    async fn watch_machine_events(
        &self,
        _request: Request<pb::WatchMachineEventsRequest>,
    ) -> Result<Response<Self::WatchMachineEventsStream>, Status> {
        Err(Status::unimplemented(
            "streaming not implemented yet: a future PR subscribes this to svc.events",
        ))
    }
}
