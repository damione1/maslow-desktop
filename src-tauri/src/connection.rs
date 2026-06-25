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

use crate::calibration;
use crate::grbl;
use crate::maslow;
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
    /// Dump the full machine config (`$CD`) and capture the streamed YAML from
    /// the WS (the HTTP `/command` endpoint returns an empty body for non-`[ESP]`
    /// commands — the output is routed to the active WS channel instead).
    DumpConfig,
}

#[derive(Default)]
pub struct ConnState {
    pub tx: Mutex<Option<UnboundedSender<WsCommand>>>,
    /// Running flag of the live connection task (a fresh one per connection,
    /// so superseded tasks shut down permanently instead of reconnecting).
    pub current: Mutex<Option<Arc<AtomicBool>>>,
    /// Set while an HTTP file upload runs. The socket loop suspends all polling
    /// (`?`/`$MINFO`/`$GSTATE`) while this is true: an SD/flash write stalls both
    /// firmware cores, and hammering it with concurrent traffic during that
    /// window can corrupt the write (the embedded UI disables its ping too).
    pub upload_active: Arc<AtomicBool>,
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
    let upload_active = state.upload_active.clone();
    tokio::spawn(async move {
        connection_loop(app, url, rx, running, upload_active).await;
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

/// Request a full config dump over the WS. The result arrives asynchronously as
/// `config-dump` (+ derived `maslow-config`/`maslow-anchors`) events.
#[tauri::command]
pub async fn request_config_dump(state: State<'_, ConnState>) -> Result<(), String> {
    send_cmd(&state, WsCommand::DumpConfig).await
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
    upload_active: Arc<AtomicBool>,
) {
    // The job lives across reconnects: a dropped socket pauses it, the next
    // socket can resume on the user's request.
    let mut job: Option<Job> = None;

    while running.load(Ordering::SeqCst) {
        match run_socket(&app, &url, &mut rx, &running, &mut job, &upload_active).await {
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

/// Recognise the short-form Maslow *action* commands (the belt / calibration /
/// stop verbs the UI sends via `send_line`). After one of these the firmware
/// state changes (or, for `$STOP`, FluidNC goes Idle while the Maslow FSM may
/// stay put) and we want the UI to reflect it fast, so we burst-poll `$GSTATE`
/// instead of waiting up to the normal 1.5 s `maslow_tick`. Polls (`$GSTATE`,
/// `$MINFO`) are deliberately NOT actions.
fn is_maslow_action(line: &str) -> bool {
    matches!(
        line.trim().to_ascii_uppercase().as_str(),
        "$ALL" | "$EXT" | "$TKSLK" | "$CAL" | "$CMP" | "$STOP" | "$ESTOP"
    )
}

/// How many fast `$GSTATE` polls to fire after an action, and how often. ~12 ×
/// 250 ms ≈ 3 s of fast feedback, after which we fall back to the 1.5 s tick.
const BURST_POLLS: u32 = 12;
const BURST_INTERVAL_MS: u64 = 250;

/// Status-report (`?`) polling cadence. Fast while the machine is moving so the
/// DRO stays smooth, slow at idle to match the embedded UI's calm rhythm (its
/// default `?` interval is 3 s) and avoid hammering the firmware while parked.
const STATUS_FAST_MS: u64 = 250;
const STATUS_IDLE_MS: u64 = 1000;

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
    upload_active: &Arc<AtomicBool>,
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
    let mut status_period = STATUS_FAST_MS;
    let mut status_tick = tokio::time::interval(Duration::from_millis(status_period));
    // Maslow telemetry poll. Skipped while a job streams: `$Maslow/getInfo`
    // returns an `ok` that would corrupt the job's char-counting.
    let mut maslow_tick = tokio::time::interval(Duration::from_millis(1500));
    // Fast post-action feedback loop: after a Maslow action command we want the
    // UI to see the new firmware state within a few hundred ms (not 1.5 s). The
    // interval runs continuously but only emits while `burst_remaining > 0`,
    // which the Line handler arms when it sees an action command.
    let mut maslow_burst = tokio::time::interval(Duration::from_millis(BURST_INTERVAL_MS));
    let mut burst_remaining: u32 = 0;
    let mut last_activity = Instant::now();
    let mut ctx = SocketCtx::default();
    // Emit an initial policy so realtime controls light up immediately.
    refresh_policy(app, &mut ctx, job.as_ref().map_or(false, |j| j.active()));

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
                                    handle_incoming(app, &line, &mut ctx, &mut write, job).await?;
                                }
                            }
                            Message::Text(t) => {
                                for raw in t.split('\n') {
                                    let line = raw.trim_end_matches('\r').to_string();
                                    handle_incoming(app, &line, &mut ctx, &mut write, job).await?;
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
                            let is_action = is_maslow_action(&l);
                            if !l.ends_with('\n') { l.push('\n'); }
                            write.send(Message::Text(l)).await.map_err(|e| e.to_string())?;
                            if is_action {
                                // Reflect the new firmware state ASAP: ask for it
                                // right now (one $MINFO + $GSTATE, queued right
                                // after the action so the firmware answers post-
                                // transition), then burst-poll $GSTATE for ~3 s.
                                // Safe here because a streaming job is excluded
                                // above — these extra `ok`s would corrupt char-
                                // counting only mid-job, which can't happen here.
                                let _ = write.send(Message::Text("$MINFO\n".to_string())).await;
                                let _ = write.send(Message::Text("$GSTATE\n".to_string())).await;
                                burst_remaining = BURST_POLLS;
                                maslow_burst.reset();
                            }
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
                    Some(WsCommand::DumpConfig) => {
                        if job.as_ref().map_or(false, |j| j.active()) {
                            let _ = app.emit("config-dump-error",
                                "stop the job to read the config".to_string());
                        } else {
                            // Capture the streamed YAML until the terminating ok;
                            // suspend the burst poll so its `ok`s can't truncate it.
                            ctx.capture = Some(Vec::new());
                            ctx.capture_started = Some(Instant::now());
                            burst_remaining = 0;
                            write.send(Message::Text("$CD\n".to_string()))
                                .await.map_err(|e| e.to_string())?;
                        }
                    }
                    None => {
                        let _ = write.send(Message::Close(None)).await;
                        return Ok(());
                    }
                }
            }
            _ = status_tick.tick() => {
                // While a file upload runs, go quiet: the SD/flash write stalls
                // both firmware cores, and concurrent socket traffic can corrupt
                // it. Keep `last_activity` fresh so the watchdog does not trip.
                if upload_active.load(Ordering::Relaxed) {
                    last_activity = Instant::now();
                } else {
                write.send(Message::Binary(vec![b'?'])).await.map_err(|e| e.to_string())?;
                // Abort a stuck config capture so polling resumes.
                if let Some(started) = ctx.capture_started {
                    if started.elapsed() > Duration::from_secs(10) {
                        ctx.capture = None;
                        ctx.capture_started = None;
                        let _ = app.emit("config-dump-error", "config dump timed out".to_string());
                    }
                }
                if last_activity.elapsed() > Duration::from_secs(20) {
                    return Err("watchdog: no activity for 20s".into());
                }
                // Adapt the `?` cadence to motion: fast while moving, calm at
                // idle. interval_at delays the first new tick by a full period
                // so changing rate does not fire an extra immediate poll.
                let moving = matches!(
                    ctx.fluidnc_state.as_str(),
                    "Run" | "Jog" | "Home" | "Homing"
                );
                let desired = if moving { STATUS_FAST_MS } else { STATUS_IDLE_MS };
                if desired != status_period {
                    status_period = desired;
                    let period = Duration::from_millis(status_period);
                    status_tick = tokio::time::interval_at(
                        tokio::time::Instant::now() + period,
                        period,
                    );
                }
                }
            }
            _ = maslow_tick.tick() => {
                if !upload_active.load(Ordering::Relaxed)
                    && ctx.capture.is_none()
                    && !job.as_ref().map_or(false, |j| j.active()) {
                    // Short command names ($MINFO/$GSTATE), as the embedded UI uses;
                    // the long `$Maslow/getInfo` form is rejected by the firmware.
                    let _ = write.send(Message::Text("$MINFO\n".to_string())).await;
                    let _ = write.send(Message::Text("$GSTATE\n".to_string())).await;
                }
            }
            _ = maslow_burst.tick(), if burst_remaining > 0 => {
                // Fast follow-up after an action so the UI converges in a few
                // hundred ms. Only $GSTATE (cheap, drives the policy); skipped
                // during a job for the same char-counting reason as maslow_tick.
                if !upload_active.load(Ordering::Relaxed)
                    && !job.as_ref().map_or(false, |j| j.active()) {
                    let _ = write.send(Message::Text("$GSTATE\n".to_string())).await;
                }
                burst_remaining -= 1;
            }
        }
    }
}

/// Classify one incoming line: feed acknowledgements to the active job (with
/// char-counting), then emit it for the console / status panel.
async fn handle_incoming(
    app: &AppHandle,
    line: &str,
    ctx: &mut SocketCtx,
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
                // Job finished: manual actions become available again.
                refresh_policy(app, ctx, false);
            } else {
                streaming::emit_progress(app, job, None);
            }
        }
    }

    let job_active = job.as_ref().map_or(false, |j| j.active());
    route_line(app, line, ctx, job_active);
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

/// Suppress invalid Maslow state reports that land within this window after a
/// state change — they are almost certainly late stragglers from reordering or
/// the 1.5 s GSTATE poll lagging behind an action we just triggered.
const STATE_DEBOUNCE_MS: u64 = 400;

/// Per-socket reconciliation state, kept across lines within one connection.
#[derive(Default)]
struct SocketCtx {
    /// FluidNC only emits WCO periodically; cache so work position stays stable.
    wco_cache: Vec<f32>,
    /// Authoritative Maslow calibration state, with transition validation.
    tracker: maslow::StateTracker,
    /// Last FluidNC machine state string (drives policy recompute on change).
    fluidnc_state: String,
    /// Last emitted action policy, to avoid spamming identical events.
    last_policy: Option<maslow::ActionPolicy>,
    /// When `Some`, a `$CD` config dump is being captured: accumulated YAML
    /// lines, awaiting the terminating `ok`/`error`.
    capture: Option<Vec<String>>,
    /// Start time of the in-flight capture, for the timeout guard.
    capture_started: Option<Instant>,
}

#[derive(serde::Serialize, Clone)]
struct Discord {
    /// "straggler" = suppressed, "accepted" = machine prevailed (state updated).
    kind: &'static str,
    from: i32,
    to: i32,
    from_label: String,
    to_label: String,
}

fn emit_discord(app: &AppHandle, kind: &'static str, from: i32, to: i32) {
    let _ = app.emit(
        "maslow-discord",
        Discord {
            kind,
            from,
            to,
            from_label: maslow::label_for(from).to_string(),
            to_label: maslow::label_for(to).to_string(),
        },
    );
}

/// Recompute the unified action policy and emit it only when it changed.
fn refresh_policy(app: &AppHandle, ctx: &mut SocketCtx, job_active: bool) {
    let p = maslow::action_policy(&ctx.fluidnc_state, ctx.tracker.current(), job_active);
    if ctx.last_policy.as_ref() != Some(&p) {
        ctx.last_policy = Some(p.clone());
        let _ = app.emit("action-policy", p);
    }
}

/// Classify and emit a single complete line, updating reconciliation state.
fn route_line(app: &AppHandle, line: &str, ctx: &mut SocketCtx, job_active: bool) {
    if line.is_empty() {
        return;
    }
    // Config dump capture: accumulate `$CD` YAML until the terminating ok/error.
    // Guards against stale poll output racing the dump: an `ok` with an empty
    // buffer is a leftover `$MINFO`/`$GSTATE` ack from just before the dump (the
    // firmware answers those in order, before `$CD`'s body), so it's ignored;
    // poll noise (`[MSG]`/`MINFO:`/control) is dropped; status reports are let
    // through to keep the DRO live; only real YAML lines are captured.
    if let Some(buf) = ctx.capture.as_mut() {
        if ack_kind(line).is_some() {
            if buf.is_empty() {
                return;
            }
            let yaml = ctx.capture.take().unwrap().join("\n");
            ctx.capture_started = None;
            finalize_config_dump(app, &yaml);
            return;
        }
        if grbl::parse_status_report(line).is_some() {
            // fall through to update machine-status; not part of the config
        } else if line.starts_with('[') || line.starts_with("MINFO:") || is_control(line) {
            return; // stale poll output, not config
        } else {
            buf.push(line.to_string());
            return;
        }
    }
    if let Some(mut status) = grbl::parse_status_report(line) {
        if !status.wco.is_empty() {
            ctx.wco_cache = status.wco.clone();
        } else if !ctx.wco_cache.is_empty() {
            status.wco = ctx.wco_cache.clone();
        }
        if !status.wco.is_empty() && !status.mpos.is_empty() {
            status.wpos = status
                .mpos
                .iter()
                .zip(&status.wco)
                .map(|(m, o)| m - o)
                .collect();
        }
        let changed = status.state != ctx.fluidnc_state;
        if changed {
            ctx.fluidnc_state = status.state.clone();
        }
        let _ = app.emit("machine-status", &status);
        if changed {
            refresh_policy(app, ctx, job_active);
        }
        return;
    }
    if let Some(info) = maslow::parse_minfo(line) {
        let _ = app.emit("maslow-info", &info);
        return;
    }
    if let Some(state) = maslow::parse_state(line) {
        match ctx.tracker.observe(state, STATE_DEBOUNCE_MS) {
            maslow::Observation::First(s) | maslow::Observation::Valid(s) => {
                let _ = app.emit("maslow-state", maslow::policy_for(s));
                refresh_policy(app, ctx, job_active);
            }
            maslow::Observation::Unchanged => {}
            maslow::Observation::Straggler { from, to } => {
                emit_discord(app, "straggler", from, to);
            }
            maslow::Observation::Discord { from, to } => {
                emit_discord(app, "accepted", from, to);
                let _ = app.emit("maslow-state", maslow::policy_for(to));
                refresh_policy(app, ctx, job_active);
            }
        }
        return;
    }
    if let Some(wp) = maslow::parse_waypoint(line) {
        let _ = app.emit("maslow-waypoint", wp);
        return;
    }
    if let Some(measurements) = calibration::parse_clbm(line) {
        // Raw belt measurements the firmware logs before each recompute — the
        // input the client-side solver re-solves from.
        let _ = app.emit("cal-measurements", measurements);
        return;
    }
    if let Some(fit) = calibration::parse_fit_line(line) {
        let _ = app.emit("cal-firmware-fit", fit);
        // fall through so the fit summary also appears in the console
    }
    if let Some(anchors) = calibration::parse_recompute_line(line) {
        let _ = app.emit("cal-firmware-anchors", anchors);
        // fall through so the result also appears in the console
    }
    if line.contains("Calibration complete") {
        let _ = app.emit("maslow-cal-complete", ());
        // fall through so the message also appears in the console
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
    if is_console_chatter(line) {
        return;
    }
    let _ = app.emit("grbl-line", line.to_string());
}

/// Periodic firmware chatter the embedded UI also hides from its (non-verbose)
/// console: the idle heartbeat ping and the belt-tension telemetry stream. These
/// are what made our console scroll at idle. Mirrors `commands.js` filters.
fn is_console_chatter(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("[MSG:INFO: Heartbeat]")
        || (t.starts_with("[MSG:INFO:")
            && t.contains("TLC:")
            && t.contains("TRC:")
            && t.contains("BLC:")
            && t.contains("BRC:"))
}

/// Flatten a captured `$CD` YAML dump and emit it as `config-dump`, plus the
/// derived Maslow config and anchors (rebuilt from the same dump via a synthetic
/// `path=value` rendering the existing parsers understand).
fn finalize_config_dump(app: &AppHandle, yaml: &str) {
    let entries = crate::fluidnc::flatten_config(yaml);
    if entries.is_empty() {
        let _ = app.emit("config-dump-error", "empty or unparseable config dump".to_string());
        return;
    }
    let synthetic: String = entries
        .iter()
        .map(|e| format!("{}={}\n", e.path, e.value))
        .collect();
    let _ = app.emit("config-dump", &entries);
    if let Some(anchors) = maslow::parse_anchors(&synthetic) {
        let _ = app.emit("maslow-anchors", &anchors);
    }
    if let Some(cfg) = maslow::parse_maslow_config(&synthetic) {
        let _ = app.emit("maslow-config", &cfg);
    }
}
