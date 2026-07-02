// Lifecycle management for the gRPC and HTTP API servers: tracks whether
// they are currently running and provides graceful start/stop via oneshot
// shutdown signals, so an enable/disable toggle or a key regeneration can
// take effect without restarting the app.

use crate::service::machine::MaslowService;
use std::sync::Arc;
use tokio::sync::{oneshot, Mutex};

pub struct ApiServerHandle {
    grpc_shutdown: Mutex<Option<oneshot::Sender<()>>>,
    http_shutdown: Mutex<Option<oneshot::Sender<()>>>,
}

impl Default for ApiServerHandle {
    fn default() -> Self {
        Self::new()
    }
}

impl ApiServerHandle {
    pub fn new() -> Self {
        Self {
            grpc_shutdown: Mutex::new(None),
            http_shutdown: Mutex::new(None),
        }
    }

    /// True if a shutdown sender is currently stored, i.e. a `start_api_servers`
    /// call hasn't been matched by a `stop_api_servers` yet. This does not
    /// detect a server task that panicked or exited on its own without going
    /// through `stop_api_servers` (e.g. a bind failure) - acceptable for now,
    /// since that path already logs to stderr and leaves the toggle in a
    /// stale "on" state that the next explicit enable/disable corrects.
    pub async fn is_running(&self) -> bool {
        self.grpc_shutdown.lock().await.is_some()
    }
}

impl MaslowService {
    /// Start both API servers if they are not already running, using the
    /// ports in the current settings.
    pub async fn start_api_servers(self: &Arc<Self>) {
        if self.api_server.is_running().await {
            return;
        }

        let (grpc_tx, grpc_rx) = oneshot::channel();
        let (http_tx, http_rx) = oneshot::channel();
        *self.api_server.grpc_shutdown.lock().await = Some(grpc_tx);
        *self.api_server.http_shutdown.lock().await = Some(http_tx);

        crate::grpc::spawn_server(Arc::clone(self), async {
            let _ = grpc_rx.await;
        });
        crate::http::spawn_server(Arc::clone(self), async {
            let _ = http_rx.await;
        });
    }

    /// Signal both servers to shut down gracefully, if running. Clears the
    /// stored senders so a subsequent `start_api_servers` spawns fresh
    /// servers on the now-freed ports.
    pub async fn stop_api_servers(&self) {
        if let Some(tx) = self.api_server.grpc_shutdown.lock().await.take() {
            let _ = tx.send(());
        }
        if let Some(tx) = self.api_server.http_shutdown.lock().await.take() {
            let _ = tx.send(());
        }
    }
}
