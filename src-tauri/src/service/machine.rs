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
    pub snapshot: crate::service::snapshot::SharedSnapshot,
    pub events: tokio::sync::broadcast::Sender<crate::service::snapshot::MachineEvent>,
}

impl MaslowService {
    pub fn new(app: AppHandle) -> Self {
        let (events, _rx) = tokio::sync::broadcast::channel(256);
        Self {
            conn: ConnState::default(),
            app,
            snapshot: std::sync::Arc::new(std::sync::RwLock::new(Default::default())),
            events,
        }
    }

    pub async fn connect(self: &Arc<Self>, host: String) -> Result<(), String> {
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
        *self.conn.connected_host.lock().await = Some(host.trim().to_string());

        let upload_active = self.conn.upload_active.clone();
        let app = self.app.clone();
        let svc = Arc::clone(self);
        tokio::spawn(async move {
            connection_loop(app, url, rx, running, upload_active, svc).await;
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

    // --- Action convenience methods ----------------------------------------
    //
    // Thin wrappers around `send_line`/`send_realtime` that build the exact
    // command string the firmware expects for each action. Kept here (rather
    // than inline in the gRPC layer) so every transport adapter (gRPC/HTTP/MCP)
    // shares the same, single source of truth for command strings, matching
    // what the frontend already sends for the equivalent UI button.

    pub async fn home(&self) -> Result<(), String> {
        self.send_line("$H".to_string()).await
    }

    pub async fn unlock(&self) -> Result<(), String> {
        self.send_line("$X".to_string()).await
    }

    /// Feed-hold: realtime `!`.
    pub async fn hold(&self) -> Result<(), String> {
        self.send_realtime(0x21).await
    }

    /// Cycle-resume: realtime `~`.
    pub async fn resume(&self) -> Result<(), String> {
        self.send_realtime(0x7e).await
    }

    /// Zero the given axes (`G10 L20 P0 ...`). An empty list means the bulk
    /// X+Y zero the frontend's "zero" button sends (Z is deliberately excluded
    /// there; zeroing Z is only ever done per-axis, one letter at a time).
    pub async fn zero(&self, axes: Vec<String>) -> Result<(), String> {
        let axes = if axes.is_empty() {
            vec!["X".to_string(), "Y".to_string()]
        } else {
            axes
        };
        let terms: Vec<String> = axes.iter().map(|a| format!("{}0", a.to_uppercase())).collect();
        self.send_line(format!("G10 L20 P0 {}", terms.join(" "))).await
    }

    pub async fn retract(&self) -> Result<(), String> {
        self.send_line("$ALL".to_string()).await
    }

    pub async fn extend(&self) -> Result<(), String> {
        self.send_line("$EXT".to_string()).await
    }

    pub async fn take_slack(&self) -> Result<(), String> {
        self.send_line("$TKSLK".to_string()).await
    }

    pub async fn comply(&self) -> Result<(), String> {
        self.send_line("$CMP".to_string()).await
    }

    pub async fn calibrate(&self) -> Result<(), String> {
        self.send_line("$CAL".to_string()).await
    }

    pub async fn stop(&self) -> Result<(), String> {
        self.send_line("$STOP".to_string()).await
    }

    /// The latching Maslow belt e-stop, distinct from the always-reachable
    /// realtime soft-reset (0x18) already covered by `send_realtime`.
    pub async fn estop(&self) -> Result<(), String> {
        self.send_line("$ESTOP".to_string()).await
    }

    /// Jog by the given relative distances (mm) at the given feed rate.
    /// Builds `$J=G91 G21 <axis terms> F<feed>`, including only the axes with
    /// a non-zero delta, concatenated with no separator between terms (e.g.
    /// `X10Y-5`), matching the frontend's jog command format.
    pub async fn jog(&self, dx: f64, dy: f64, dz: f64, feed: f64) -> Result<(), String> {
        let mut terms = String::new();
        if dx != 0.0 {
            terms.push_str(&format!("X{dx}"));
        }
        if dy != 0.0 {
            terms.push_str(&format!("Y{dy}"));
        }
        if dz != 0.0 {
            terms.push_str(&format!("Z{dz}"));
        }
        if terms.is_empty() {
            return Err("jog requires at least one non-zero axis".to_string());
        }
        self.send_line(format!("$J=G91 G21 {terms} F{feed}")).await
    }
}
