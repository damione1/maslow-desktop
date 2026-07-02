// G-code job streaming: starting/pausing/resuming/stopping the active job, and
// recovering a job persisted to disk across an app restart. Tauri commands in
// `connection.rs` are one-line delegates to these methods.

use crate::connection::WsCommand;
use crate::service::machine::MaslowService;
use crate::streaming;

impl MaslowService {
    /// Begin streaming a G-code file. Loaded and parsed here so the frontend only
    /// passes a path. `start_index` lets a previous job resume mid-file. The file
    /// read runs on a blocking thread so a large file can't stall the async
    /// runtime that also drives the WebSocket connection loop.
    pub async fn start_job(&self, path: String, start_index: usize) -> Result<usize, String> {
        let read_path = path.clone();
        let lines = tauri::async_runtime::spawn_blocking(move || streaming::load_gcode(&read_path))
            .await
            .map_err(|e| format!("stream_start join: {e}"))??;
        let total = lines.len();
        self.send_cmd(WsCommand::StartJob {
            lines,
            path,
            start_index,
        })
        .await?;
        Ok(total)
    }

    pub async fn pause_job(&self) -> Result<(), String> {
        self.send_cmd(WsCommand::PauseJob).await
    }

    pub async fn resume_job(&self) -> Result<(), String> {
        self.send_cmd(WsCommand::ResumeJob).await
    }

    pub async fn stop_job(&self) -> Result<(), String> {
        self.send_cmd(WsCommand::StopJob).await
    }

    /// Return a previously interrupted job persisted on disk, if resumable.
    pub fn stream_saved(&self) -> Option<streaming::SavedJob> {
        streaming::read_saved(&self.app)
    }
}
