// MCP JobService tools, mirroring `http/job.rs`. "jobs/current" is a
// singleton: GetJob and the mutating actions all report the latest cached job
// progress, or an "idle" Job when nothing has ever streamed.

use crate::mcp::{err, ok_json, McpServer};
use crate::proto::maslow::v1 as pb;
use crate::service::machine::MaslowService;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Deserialize, JsonSchema)]
pub struct StartJobParams {
    /// Path to the G-code file on the machine's SD card, e.g. "/job.nc".
    pub path: String,
    /// Line index to resume from. Use a saved job's `acked` count (from
    /// get_saved_job) to resume an interrupted job; 0 starts from the beginning.
    #[serde(default)]
    pub start_index: u64,
}

/// Latest known job state, or an idle placeholder when no job has ever run on
/// this connection. Matches `http::job::current_job` / `JobServiceImpl::current_job`.
fn current_job(svc: &MaslowService) -> pb::Job {
    let snap = svc.snapshot.read().unwrap();
    snap.job_progress.clone().map(Into::into).unwrap_or_else(|| pb::Job {
        state: "idle".to_string(),
        ..Default::default()
    })
}

#[tool_router(router = tool_router_job, vis = "pub(crate)")]
impl McpServer {
    #[tool(
        description = "Get the state of the current (or most recently run) G-code job: state, progress counters, path. Read-only; poll this instead of subscribing to progress events."
    )]
    async fn get_job(&self) -> CallToolResult {
        ok_json(&current_job(&self.svc))
    }

    #[tool(
        description = "Get the on-disk record of a job interrupted by a disconnect or app restart, if any. Resumable via start_job with the same path and this record's acked count as start_index. Read-only."
    )]
    async fn get_saved_job(&self) -> CallToolResult {
        let saved_job = self.svc.stream_saved().map(Into::into);
        ok_json(&pb::GetSavedJobResponse { saved_job })
    }

    #[tool(
        description = "Start streaming a G-code job from the machine's SD card. The machine will physically begin cutting; only safe when get_action_policy reports run as true (belts tensioned and idle)."
    )]
    async fn start_job(&self, Parameters(req): Parameters<StartJobParams>) -> CallToolResult {
        match self.svc.start_job(req.path, req.start_index as usize).await {
            Ok(total) => ok_json(&pb::StartJobResponse { total: total as u64 }),
            Err(e) => err(e),
        }
    }

    #[tool(
        description = "Pause the currently streaming G-code job (feed-hold). The machine will physically stop in place."
    )]
    async fn pause_job(&self) -> CallToolResult {
        match self.svc.pause_job().await {
            Ok(()) => ok_json(&current_job(&self.svc)),
            Err(e) => err(e),
        }
    }

    #[tool(description = "Resume a paused G-code job. The machine will physically resume cutting.")]
    async fn resume_job(&self) -> CallToolResult {
        match self.svc.resume_job().await {
            Ok(()) => ok_json(&current_job(&self.svc)),
            Err(e) => err(e),
        }
    }

    #[tool(
        description = "Stop the currently streaming G-code job. The machine will physically stop; the job cannot be resumed from this point without a fresh start_job call."
    )]
    async fn stop_job(&self) -> CallToolResult {
        match self.svc.stop_job().await {
            Ok(()) => ok_json(&current_job(&self.svc)),
            Err(e) => err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_job_params_default_start_index_to_zero() {
        let params: StartJobParams = serde_json::from_value(serde_json::json!({"path": "/job.nc"})).unwrap();
        assert_eq!(params.path, "/job.nc");
        assert_eq!(params.start_index, 0);
    }

    #[test]
    fn start_job_params_accepts_resume_index() {
        let params: StartJobParams =
            serde_json::from_value(serde_json::json!({"path": "/job.nc", "start_index": 250})).unwrap();
        assert_eq!(params.start_index, 250);
    }
}
