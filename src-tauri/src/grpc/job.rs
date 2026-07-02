// gRPC JobService implementation. "jobs/current" is a singleton: GetJob and
// the mutating actions all report the latest cached job progress, or an
// "idle" Job when nothing has ever streamed.

use crate::grpc::stream;
use crate::proto::maslow::v1 as pb;
use crate::proto::maslow::v1::job_service_server::JobService;
use crate::service::machine::MaslowService;
use futures_util::StreamExt;
use std::pin::Pin;
use std::sync::Arc;
use tonic::{Request, Response, Status};

pub struct JobServiceImpl {
    pub svc: Arc<MaslowService>,
}

impl JobServiceImpl {
    /// Latest known job state, or an idle placeholder when no job has ever
    /// run on this connection. Chosen over a `not_found` error here because
    /// "no job right now" is the expected steady state of this singleton
    /// resource, not an exceptional one.
    fn current_job(&self) -> pb::Job {
        let snap = self.svc.snapshot.read().unwrap();
        snap.job_progress
            .clone()
            .map(Into::into)
            .unwrap_or_else(|| pb::Job {
                state: "idle".to_string(),
                ..Default::default()
            })
    }
}

#[tonic::async_trait]
impl JobService for JobServiceImpl {
    async fn get_job(&self, _request: Request<pb::GetJobRequest>) -> Result<Response<pb::Job>, Status> {
        Ok(Response::new(self.current_job()))
    }

    async fn get_saved_job(
        &self,
        _request: Request<pb::GetSavedJobRequest>,
    ) -> Result<Response<pb::GetSavedJobResponse>, Status> {
        let saved_job = self.svc.stream_saved().map(Into::into);
        Ok(Response::new(pb::GetSavedJobResponse { saved_job }))
    }

    async fn start_job(
        &self,
        request: Request<pb::StartJobRequest>,
    ) -> Result<Response<pb::StartJobResponse>, Status> {
        let r = request.into_inner();
        let total = self
            .svc
            .start_job(r.path, r.start_index as usize)
            .await
            .map_err(Status::internal)?;
        Ok(Response::new(pb::StartJobResponse { total: total as u64 }))
    }

    async fn pause_job(&self, _request: Request<pb::PauseJobRequest>) -> Result<Response<pb::Job>, Status> {
        self.svc.pause_job().await.map_err(Status::internal)?;
        Ok(Response::new(self.current_job()))
    }

    async fn resume_job(&self, _request: Request<pb::ResumeJobRequest>) -> Result<Response<pb::Job>, Status> {
        self.svc.resume_job().await.map_err(Status::internal)?;
        Ok(Response::new(self.current_job()))
    }

    async fn stop_job(&self, _request: Request<pb::StopJobRequest>) -> Result<Response<pb::Job>, Status> {
        self.svc.stop_job().await.map_err(Status::internal)?;
        Ok(Response::new(self.current_job()))
    }

    type WatchJobProgressStream = Pin<Box<dyn tonic::codegen::tokio_stream::Stream<Item = Result<pb::Job, Status>> + Send>>;

    async fn watch_job_progress(
        &self,
        _request: Request<pb::WatchJobProgressRequest>,
    ) -> Result<Response<Self::WatchJobProgressStream>, Status> {
        let rx = self.svc.events.subscribe();
        let jobs = stream::job_progress_stream(rx).map(Ok);
        Ok(Response::new(Box::pin(jobs)))
    }
}
