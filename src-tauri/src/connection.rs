// WebSocket connection manager for the Maslow / FluidNC machine.
// Connects to ws://<host>:81/ (subprotocol "arduino"), mirroring socket.js:
//   - Binary frames carry the GRBL stream (lines terminated by '\n').
//   - Text frames carry control messages "key:value" (CURRENT_ID, PING, ...).
//
// G-code streaming lives here too: the active `Job` is owned by the connection
// supervisor (`connection_loop`) so it survives automatic reconnects, while the
// streaming module persists its progress to disk for full app-restart recovery.
//
// Emits Tauri events to the frontend:
//   "ws-state"       -> "connected" | "disconnected"
//   "grbl-line"      -> raw line string (console)
//   "machine-status" -> MachineStatus JSON (parsed status report)
//   "ws-control"     -> control message string
//   "stream-progress"-> streaming::Progress JSON

use crate::grbl;
use crate::streaming::{self, Job};
use futures_util::stream::SplitSink;
use futures_util::{SinkExt, StreamExt};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, State};
use tokio::net::TcpStream;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

type WsSink = SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>;

/// Commands the frontend can push down to the live socket task.
pub enum WsCommand {
    /// A line of G-code / `$` command (a trailing newline is added).
    Line(String),
    /// A single realtime character (e.g. '?', '!', '~', 0x18) sent as-is.
    Realtime(u8),
    /// Start streaming a parsed G-code file from `start_index`.
    StartJob {
        lines: Vec<String>,
        path: String,
        start_index: usize,
    },
    PauseJob,
    ResumeJob,
    StopJob,
}

#[derive(Default)]
pub struct ConnState {
    pub tx: Mutex<Option<UnboundedSender<WsCommand>>>,
    /// Running flag of the live connection task (a fresh one per connection,
    /// so superseded tasks shut down permanently instead of reconnecting).
    pub current: Mutex<Option<Arc<AtomicBool>>>,
}

/// Build the WebSocket URL. FluidNC serves the socket on (web port + 1),
/// i.e. port 81 by default, at the root path — verified against a real Maslow M4.
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

#[tauri::command]
pub async fn connect_ws(
    app: AppHandle,
    state: State<'_, ConnState>,
    host: String,
) -> Result<(), String> {
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
pub async fn disconnect_ws(state: State<'_, ConnState>) -> Result<(), String> {
    if let Some(flag) = state.current.lock().await.take() {
        flag.store(false, Ordering::SeqCst);
    }
    *state.tx.lock().await = None;
    Ok(())
}

async fn send_cmd(state: &State<'_, ConnState>, cmd: WsCommand) -> Result<(), String> {
    let guard = state.tx.lock().await;
    match guard.as_ref() {
        Some(tx) => tx.send(cmd).map_err(|e| e.to_string()),
        None => Err("not connected".into()),
    }
}

#[tauri::command]
pub async fn send_line(state: State<'_, ConnState>, line: String) -> Result<(), String> {
    send_cmd(&state, WsCommand::Line(line)).await
}

#[tauri::command]
pub async fn send_realtime(state: State<'_, ConnState>, byte: u8) -> Result<(), String> {
    send_cmd(&state, WsCommand::Realtime(byte)).await
}

/// Begin streaming a G-code file. Loaded and parsed here so the frontend only
/// passes a path. `start_index` lets a previous job resume mid-file.
#[tauri::command]
pub async fn stream_start(
    state: State<'_, ConnState>,
    path: String,
    start_index: usize,
) -> Result<usize, String> {
    let lines = streaming::load_gcode(&path)?;
    let total = lines.len();
    send_cmd(
        &state,
        WsCommand::StartJob {
            lines,
            path,
            start_index,
        },
    )
    .await?;
    Ok(total)
}

#[tauri::command]
pub async fn stream_pause(state: State<'_, ConnState>) -> Result<(), String> {
    send_cmd(&state, WsCommand::PauseJob).await
}

#[tauri::command]
pub async fn stream_resume(state: State<'_, ConnState>) -> Result<(), String> {
    send_cmd(&state, WsCommand::ResumeJob).await
}

#[tauri::command]
pub async fn stream_stop(state: State<'_, ConnState>) -> Result<(), String> {
    send_cmd(&state, WsCommand::StopJob).await
}

/// Return a previously interrupted job persisted on disk, if resumable.
#[tauri::command]
pub fn stream_saved(app: AppHandle) -> Option<streaming::SavedJob> {
    streaming::read_saved(&app)
}

async fn connection_loop(
    app: AppHandle,
    url: String,
    mut rx: UnboundedReceiver<WsCommand>,
    running: Arc<AtomicBool>,
) {
    // The job lives across reconnects: a dropped socket pauses it, the next
    // socket can resume on the user's request.
    let mut job: Option<Job> = None;

    while running.load(Ordering::SeqCst) {
        match run_socket(&app, &url, &mut rx, &running, &mut job).await {
            Ok(_) => {}
            Err(e) => {
                let _ = app.emit("ws-error", e);
            }
        }
        let _ = app.emit("ws-state", "disconnected");

        // A live job whose socket just dropped is now interrupted: freeze it at
        // the last acknowledged line (firmware buffer state is unknown).
        if let Some(j) = job.as_mut() {
            if j.active() {
                j.invalidate_inflight();
                streaming::persist(&app, j, "interrupted");
                streaming::emit_progress(&app, &job, Some("interrupted"));
            }
        }

        if !running.load(Ordering::SeqCst) {
            break;
        }
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
    let _ = app.emit("ws-state", "disconnected");
}

/// Send as many queued G-code lines as the firmware RX buffer allows.
async fn pump(write: &mut WsSink, job: &mut Option<Job>) -> Result<(), String> {
    if let Some(j) = job.as_mut() {
        while let Some(line) = j.next_line() {
            write
                .send(Message::Text(line))
                .await
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

async fn run_socket(
    app: &AppHandle,
    url: &str,
    rx: &mut UnboundedReceiver<WsCommand>,
    running: &Arc<AtomicBool>,
    job: &mut Option<Job>,
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
                            Message::Binary(b) => {
                                buf.push_str(&String::from_utf8_lossy(&b));
                                while let Some(idx) = buf.find('\n') {
                                    let raw: String = buf.drain(..=idx).collect();
                                    let line = raw.trim_end_matches(['\r', '\n']).to_string();
                                    handle_incoming(app, &line, &mut wco_cache, &mut write, job).await?;
                                }
                            }
                            Message::Text(t) => {
                                for raw in t.split('\n') {
                                    let line = raw.trim_end_matches('\r').to_string();
                                    handle_incoming(app, &line, &mut wco_cache, &mut write, job).await?;
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
                        if job.as_ref().map_or(false, |j| j.active()) {
                            let _ = app.emit("grbl-line",
                                "[blocked] stop the job to send manual commands".to_string());
                        } else {
                            if !l.ends_with('\n') { l.push('\n'); }
                            write.send(Message::Text(l)).await.map_err(|e| e.to_string())?;
                        }
                    }
                    Some(WsCommand::Realtime(b)) => {
                        write.send(Message::Binary(vec![b])).await.map_err(|e| e.to_string())?;
                        // Ctrl-X soft reset flushes the firmware buffer: any active
                        // job's in-flight tracking is now stale.
                        if b == 0x18 {
                            if let Some(j) = job.as_mut() {
                                j.invalidate_inflight();
                                streaming::persist(app, j, "interrupted");
                            }
                            streaming::emit_progress(app, job, Some("interrupted"));
                        }
                    }
                    Some(WsCommand::StartJob { lines, path, start_index }) => {
                        *job = Some(Job::new(path, lines, start_index));
                        if let Some(j) = job.as_mut() {
                            streaming::persist(app, j, "running");
                        }
                        pump(&mut write, job).await?;
                        streaming::emit_progress(app, job, None);
                    }
                    Some(WsCommand::PauseJob) => {
                        if let Some(j) = job.as_mut() {
                            j.paused = true;
                            streaming::persist(app, j, "interrupted");
                        }
                        // Feed hold so motion stops promptly, not just queueing.
                        let _ = write.send(Message::Binary(vec![b'!'])).await;
                        streaming::emit_progress(app, job, None);
                    }
                    Some(WsCommand::ResumeJob) => {
                        if let Some(j) = job.as_mut() {
                            j.paused = false;
                        }
                        // Release a possible feed hold before refilling the buffer.
                        let _ = write.send(Message::Binary(vec![b'~'])).await;
                        pump(&mut write, job).await?;
                        streaming::emit_progress(app, job, None);
                    }
                    Some(WsCommand::StopJob) => {
                        streaming::clear_saved(app);
                        *job = None;
                        streaming::emit_progress(app, job, None);
                    }
                    None => {
                        let _ = write.send(Message::Close(None)).await;
                        return Ok(());
                    }
                }
            }
            _ = status_tick.tick() => {
                write.send(Message::Binary(vec![b'?'])).await.map_err(|e| e.to_string())?;
                if last_activity.elapsed() > Duration::from_secs(20) {
                    return Err("watchdog: no activity for 20s".into());
                }
            }
        }
    }
}

/// Classify one incoming line: feed acknowledgements to the active job (with
/// char-counting), then emit it for the console / status panel.
async fn handle_incoming(
    app: &AppHandle,
    line: &str,
    wco_cache: &mut Vec<f32>,
    write: &mut WsSink,
    job: &mut Option<Job>,
) -> Result<(), String> {
    if let Some(is_error) = ack_kind(line) {
        if job.as_ref().map_or(false, |j| j.active()) {
            let done = job.as_mut().map(|j| j.ack(is_error)).unwrap_or(false);
            if let Some(j) = job.as_mut() {
                streaming::maybe_persist(app, j);
            }
            // Buffer space may have freed up: keep the pipe full.
            pump(write, job).await?;
            if done {
                let final_state = job
                    .as_ref()
                    .map(|j| if j.errors > 0 { "error" } else { "done" })
                    .unwrap_or("done");
                if let Some(j) = job.as_mut() {
                    streaming::persist(app, j, final_state);
                }
                streaming::emit_progress(app, job, None);
                streaming::clear_saved(app);
                *job = None;
            } else {
                streaming::emit_progress(app, job, None);
            }
        }
    }

    dispatch_line(app, line, wco_cache);
    Ok(())
}

/// Recognise a GRBL acknowledgement line. `Some(true)` = error, `Some(false)` = ok.
fn ack_kind(line: &str) -> Option<bool> {
    let t = line.trim();
    if t == "ok" {
        Some(false)
    } else if t.starts_with("error") {
        Some(true)
    } else {
        None
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

/// Classify and emit a single complete line (console + status).
fn dispatch_line(app: &AppHandle, line: &str, wco_cache: &mut Vec<f32>) {
    if line.is_empty() {
        return;
    }
    if let Some(mut status) = grbl::parse_status_report(line) {
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
