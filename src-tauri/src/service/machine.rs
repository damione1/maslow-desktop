// Owns the live connection to the machine and every action that touches it:
// connect/disconnect, sending lines/realtime bytes, writing settings, saving
// config, and requesting a config dump. Tauri commands in `connection.rs` are
// one-line delegates to these methods; later transport adapters (gRPC/HTTP/MCP)
// will call the exact same methods.

use crate::connection::{connection_loop, ConnState, WsCommand};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::AppHandle;
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

/// Timeout for a tracked write's firmware reply. FluidNC answers in order and
/// well within this window in normal operation; a slower answer usually means
/// the socket has gone quiet.
const TRACKED_WRITE_TIMEOUT: Duration = Duration::from_secs(3);

/// Build the WebSocket URL. FluidNC serves the socket on (web port + 1),
/// i.e. port 81 by default, at the root path, verified against a real Maslow M4.
fn ws_url(host: &str) -> String {
    let h = host
        .trim()
        .trim_end_matches('/')
        .trim_start_matches("http://")
        .trim_start_matches("https://");
    if h.contains(':') {
        format!("ws://{}/", h)
    } else {
        format!("ws://{}:81/", h)
    }
}

pub struct MaslowService {
    pub conn: ConnState,
    pub app: AppHandle,
}

impl MaslowService {
    pub fn new(app: AppHandle) -> Self {
        Self {
            conn: ConnState::default(),
            app,
        }
    }

    pub async fn connect(&self, host: String) -> Result<(), String> {
        // Reject a malformed host up front. A bad URL can never connect, and the
        // reconnect loop would otherwise retry it every few seconds and spam the
        // console with "invalid uri character". Validate before touching any
        // existing connection so a typo cannot drop a working one.
        let url = ws_url(&host);
        url.as_str()
            .into_client_request()
            .map_err(|e| format!("invalid host \"{}\": {e}", host.trim()))?;

        if let Some(flag) = self.conn.current.lock().await.take() {
            flag.store(false, Ordering::SeqCst);
        }

        let running = Arc::new(AtomicBool::new(true));
        *self.conn.current.lock().await = Some(running.clone());

        let (tx, rx) = mpsc::unbounded_channel::<WsCommand>();
        *self.conn.tx.lock().await = Some(tx);

        let upload_active = self.conn.upload_active.clone();
        let app = self.app.clone();
        tokio::spawn(async move {
            connection_loop(app, url, rx, running, upload_active).await;
        });
        Ok(())
    }

    pub async fn disconnect(&self) -> Result<(), String> {
        if let Some(flag) = self.conn.current.lock().await.take() {
            flag.store(false, Ordering::SeqCst);
        }
        *self.conn.tx.lock().await = None;
        Ok(())
    }

    pub(crate) async fn send_cmd(&self, cmd: WsCommand) -> Result<(), String> {
        let guard = self.conn.tx.lock().await;
        match guard.as_ref() {
            Some(tx) => tx.send(cmd).map_err(|e| e.to_string()),
            None => Err("not connected".into()),
        }
    }

    pub async fn send_line(&self, line: String) -> Result<(), String> {
        self.send_cmd(WsCommand::Line(line)).await
    }

    pub async fn send_realtime(&self, byte: u8) -> Result<(), String> {
        self.send_cmd(WsCommand::Realtime(byte)).await
    }

    /// Send a line over the WS and wait for the firmware's own `ok`/`error:N` reply,
    /// rather than the HTTP `/command` endpoint's body (which is empty for anything
    /// other than `[ESP...]` commands: FluidNC forwards `$`/`$/` output to the WS
    /// channel matching the request's page id instead of the HTTP response).
    pub(crate) async fn tracked_send(&self, line: String) -> Result<(), String> {
        let (reply, rx) = oneshot::channel();
        self.send_cmd(WsCommand::TrackedLine { line, reply }).await?;
        match tokio::time::timeout(TRACKED_WRITE_TIMEOUT, rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err("connection closed before the machine replied".to_string()),
            Err(_) => Err("timeout waiting for machine reply".to_string()),
        }
    }

    /// Write a single FluidNC setting: `$/<path>=<value>`. `path` is the full
    /// config path (e.g. `Maslow_Work_Area_X` or `kinematics/MaslowKinematics/tlX`).
    pub async fn write_setting(&self, path: String, value: String) -> Result<(), String> {
        self.tracked_send(format!("$/{path}={value}")).await
    }

    /// Persist the current (runtime-edited) config to flash via `$CO`
    /// (Config/Overwrite). Without this, edited settings are lost on reboot.
    pub async fn save_config(&self) -> Result<(), String> {
        self.tracked_send("$CO".to_string()).await
    }

    /// Request a full config dump over the WS. The result arrives asynchronously as
    /// `config-dump` (+ derived `maslow-anchors`) events.
    pub async fn request_config_dump(&self) -> Result<(), String> {
        self.send_cmd(WsCommand::DumpConfig).await
    }
}
