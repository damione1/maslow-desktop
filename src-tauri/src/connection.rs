// WebSocket connection manager for the Maslow / FluidNC machine.
// Connects to ws://<host>/ws (subprotocol "arduino"), mirroring socket.js:
//   - Binary frames carry the GRBL stream (lines terminated by '\n').
//   - Text frames carry control messages "key:value" (CURRENT_ID, PING, ...).
// Emits Tauri events to the frontend:
//   "ws-state"      -> "connected" | "disconnected"
//   "grbl-line"     -> raw line string (console)
//   "machine-status"-> MachineStatus JSON (parsed status report)
//   "ws-control"    -> control message string

use crate::grbl;
use futures_util::{SinkExt, StreamExt};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;

/// Commands the frontend can push down to the live socket task.
pub enum WsCommand {
    /// A line of G-code / `$` command (a trailing newline is added).
    Line(String),
    /// A single realtime character (e.g. '?', '!', '~', 0x18) sent as-is.
    Realtime(u8),
}

#[derive(Default)]
pub struct ConnState {
    pub tx: Mutex<Option<UnboundedSender<WsCommand>>>,
    /// Running flag of the live connection task (a fresh one per connection,
    /// so superseded tasks shut down permanently instead of reconnecting).
    pub current: Mutex<Option<Arc<AtomicBool>>>,
    /// Latest CURRENT_ID reported by the firmware (PAGEID for /command).
    pub page_id: Mutex<String>,
}

/// Build the WebSocket URL. FluidNC serves the socket on (web port + 1),
/// i.e. port 81 by default, at the root path — verified against a real Maslow M4.
fn ws_url(host: &str) -> String {
    let h = host
        .trim()
        .trim_end_matches('/')
        .trim_start_matches("http://")
        .trim_start_matches("https://");
    // If the user already specified an explicit port, respect it.
    if h.contains(':') {
        format!("ws://{}/", h)
    } else {
        format!("ws://{}:81/", h)
    }
}

#[tauri::command]
pub async fn connect_ws(
    app: AppHandle,
    state: tauri::State<'_, ConnState>,
    host: String,
) -> Result<(), String> {
    // Stop any previous connection task permanently.
    if let Some(flag) = state.current.lock().await.take() {
        flag.store(false, Ordering::SeqCst);
    }

    let running = Arc::new(AtomicBool::new(true));
    *state.current.lock().await = Some(running.clone());

    let (tx, rx) = mpsc::unbounded_channel::<WsCommand>();
    *state.tx.lock().await = Some(tx);

    let url = ws_url(&host);
    tokio::spawn(async move {
        connection_loop(app, url, rx, running).await;
    });
    Ok(())
}

#[tauri::command]
pub async fn disconnect_ws(state: tauri::State<'_, ConnState>) -> Result<(), String> {
    if let Some(flag) = state.current.lock().await.take() {
        flag.store(false, Ordering::SeqCst);
    }
    *state.tx.lock().await = None;
    Ok(())
}

#[tauri::command]
pub async fn send_line(state: tauri::State<'_, ConnState>, line: String) -> Result<(), String> {
    let guard = state.tx.lock().await;
    match guard.as_ref() {
        Some(tx) => tx.send(WsCommand::Line(line)).map_err(|e| e.to_string()),
        None => Err("not connected".into()),
    }
}

#[tauri::command]
pub async fn send_realtime(state: tauri::State<'_, ConnState>, byte: u8) -> Result<(), String> {
    let guard = state.tx.lock().await;
    match guard.as_ref() {
        Some(tx) => tx.send(WsCommand::Realtime(byte)).map_err(|e| e.to_string()),
        None => Err("not connected".into()),
    }
}

async fn connection_loop(
    app: AppHandle,
    url: String,
    mut rx: UnboundedReceiver<WsCommand>,
    running: Arc<AtomicBool>,
) {
    while running.load(Ordering::SeqCst) {
        match run_socket(&app, &url, &mut rx, &running).await {
            Ok(_) => {}
            Err(e) => {
                let _ = app.emit("ws-error", e);
            }
        }
        let _ = app.emit("ws-state", "disconnected");
        if !running.load(Ordering::SeqCst) {
            break;
        }
        // Retry every 3s, like socket.js.
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
    let _ = app.emit("ws-state", "disconnected");
}

async fn run_socket(
    app: &AppHandle,
    url: &str,
    rx: &mut UnboundedReceiver<WsCommand>,
    running: &Arc<AtomicBool>,
) -> Result<(), String> {
    let mut request = url.into_client_request().map_err(|e| e.to_string())?;
    request
        .headers_mut()
        .insert("Sec-WebSocket-Protocol", "arduino".parse().unwrap());

    let (ws, _) = tokio_tungstenite::connect_async(request)
        .await
        .map_err(|e| e.to_string())?;
    let (mut write, mut read) = ws.split();

    let _ = app.emit("ws-state", "connected");

    let mut buf = String::new();
    let mut status_tick = tokio::time::interval(Duration::from_millis(250));
    let mut last_activity = Instant::now();
    // FluidNC only emits WCO periodically; cache it so work position stays stable.
    let mut wco_cache: Vec<f32> = Vec::new();

    loop {
        if !running.load(Ordering::SeqCst) {
            let _ = write.send(Message::Close(None)).await;
            return Ok(());
        }

        tokio::select! {
            incoming = read.next() => {
                match incoming {
                    Some(Ok(msg)) => {
                        last_activity = Instant::now();
                        match msg {
                            // Binary frames carry the newline-delimited GRBL stream.
                            Message::Binary(b) => {
                                buf.push_str(&String::from_utf8_lossy(&b));
                                drain_lines(app, &mut buf, &mut wco_cache);
                            }
                            // Text frames carry control messages (CURRENT_ID, PING…)
                            // and, on some firmwares, complete GRBL lines.
                            Message::Text(t) => {
                                for raw in t.split('\n') {
                                    dispatch_line(app, raw.trim_end_matches('\r'), &mut wco_cache);
                                }
                            }
                            Message::Close(_) => return Ok(()),
                            _ => {}
                        }
                    }
                    Some(Err(e)) => return Err(e.to_string()),
                    None => return Ok(()),
                }
            }
            cmd = rx.recv() => {
                match cmd {
                    Some(WsCommand::Line(mut l)) => {
                        if !l.ends_with('\n') { l.push('\n'); }
                        write.send(Message::Text(l)).await.map_err(|e| e.to_string())?;
                    }
                    Some(WsCommand::Realtime(b)) => {
                        write.send(Message::Binary(vec![b])).await.map_err(|e| e.to_string())?;
                    }
                    None => {
                        // sender dropped -> disconnect requested
                        let _ = write.send(Message::Close(None)).await;
                        return Ok(());
                    }
                }
            }
            _ = status_tick.tick() => {
                // Poll for a realtime status report.
                write.send(Message::Binary(vec![b'?'])).await.map_err(|e| e.to_string())?;
                // Watchdog: no traffic for 20s -> force reconnect (socket.js parity).
                if last_activity.elapsed() > Duration::from_secs(20) {
                    return Err("watchdog: no activity for 20s".into());
                }
            }
        }
    }
}

/// Split accumulated buffer into complete lines and dispatch each.
fn drain_lines(app: &AppHandle, buf: &mut String, wco_cache: &mut Vec<f32>) {
    while let Some(idx) = buf.find('\n') {
        let line: String = buf.drain(..=idx).collect();
        dispatch_line(app, line.trim_end_matches(['\r', '\n']), wco_cache);
    }
}

/// A control message is "KEY:..." for a known uppercase key, not a `[MSG:...]`
/// bracketed GRBL message.
fn is_control(line: &str) -> bool {
    !line.starts_with('[')
        && matches!(
            line.split(':').next(),
            Some("CURRENT_ID" | "ACTIVE_ID" | "PING" | "DHT" | "ERROR" | "MSG")
        )
}

/// Classify and emit a single complete line.
fn dispatch_line(app: &AppHandle, line: &str, wco_cache: &mut Vec<f32>) {
    if line.is_empty() {
        return;
    }
    if let Some(mut status) = grbl::parse_status_report(line) {
        // Apply the persistent WCO so work position is stable even on reports
        // that omit WCO (FluidNC sends it only periodically).
        if !status.wco.is_empty() {
            *wco_cache = status.wco.clone();
        } else if !wco_cache.is_empty() {
            status.wco = wco_cache.clone();
        }
        if !status.wco.is_empty() && !status.mpos.is_empty() {
            status.wpos = status
                .mpos
                .iter()
                .zip(&status.wco)
                .map(|(m, o)| m - o)
                .collect();
        }
        let _ = app.emit("machine-status", &status);
        return;
    }
    if is_control(line) {
        let _ = app.emit("ws-control", line.to_string());
        if let Some(key) = line.split(':').next() {
            if key == "CURRENT_ID" || key == "ACTIVE_ID" {
                let _ = app.emit("ws-pageid", line.to_string());
            }
        }
        return;
    }
    let _ = app.emit("grbl-line", line.to_string());
}
