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
use crate::service::machine::MaslowService;
use crate::service::snapshot::{publish, MachineEvent, WsState};
use crate::streaming::{self, Job};
use futures_util::stream::SplitSink;
use futures_util::{SinkExt, StreamExt};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, State};
use tokio::net::TcpStream;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::sync::{oneshot, Mutex};
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
    /// A `$`/`$/` line whose firmware `ok`/`error:N` reply is tracked and
    /// reported back to the caller (used for setting writes and `$CO`, whose
    /// HTTP counterpart cannot see the real result: FluidNC routes the answer
    /// to the WS channel that owns the page id, not the HTTP body).
    TrackedLine {
        line: String,
        reply: oneshot::Sender<Result<(), String>>,
    },
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

#[tauri::command]
pub async fn connect_ws(state: State<'_, Arc<MaslowService>>, host: String) -> Result<(), String> {
    state.connect(host).await
}

#[tauri::command]
pub async fn disconnect_ws(state: State<'_, Arc<MaslowService>>) -> Result<(), String> {
    state.disconnect().await
}

#[tauri::command]
pub async fn send_line(state: State<'_, Arc<MaslowService>>, line: String) -> Result<(), String> {
    state.send_line(line).await
}

#[tauri::command]
pub async fn send_realtime(state: State<'_, Arc<MaslowService>>, byte: u8) -> Result<(), String> {
    state.send_realtime(byte).await
}

#[tauri::command]
pub async fn ws_write_setting(
    state: State<'_, Arc<MaslowService>>,
    path: String,
    value: String,
) -> Result<(), String> {
    state.write_setting(path, value).await
}

#[tauri::command]
pub async fn ws_save_config(state: State<'_, Arc<MaslowService>>) -> Result<(), String> {
    state.save_config().await
}

#[tauri::command]
pub async fn request_config_dump(state: State<'_, Arc<MaslowService>>) -> Result<(), String> {
    state.request_config_dump().await
}

#[tauri::command]
pub async fn stream_start(
    state: State<'_, Arc<MaslowService>>,
    path: String,
    start_index: usize,
) -> Result<usize, String> {
    state.start_job(path, start_index).await
}

#[tauri::command]
pub async fn stream_pause(state: State<'_, Arc<MaslowService>>) -> Result<(), String> {
    state.pause_job().await
}

#[tauri::command]
pub async fn stream_resume(state: State<'_, Arc<MaslowService>>) -> Result<(), String> {
    state.resume_job().await
}

#[tauri::command]
pub async fn stream_stop(state: State<'_, Arc<MaslowService>>) -> Result<(), String> {
    state.stop_job().await
}

#[tauri::command]
pub fn stream_saved(state: State<'_, Arc<MaslowService>>) -> Option<streaming::SavedJob> {
    state.stream_saved()
}

pub(crate) async fn connection_loop(
    app: AppHandle,
    url: String,
    mut rx: UnboundedReceiver<WsCommand>,
    running: Arc<AtomicBool>,
    upload_active: Arc<AtomicBool>,
    svc: Arc<MaslowService>,
) {
    // The job lives across reconnects: a dropped socket pauses it, the next
    // socket can resume on the user's request.
    let mut job: Option<Job> = None;

    while running.load(Ordering::SeqCst) {
        match run_socket(&app, &url, &mut rx, &running, &mut job, &upload_active, &svc).await {
            Ok(_) => {}
            Err(e) => {
                publish(&svc, MachineEvent::WsError(e.clone()));
                let _ = app.emit("ws-error", e);
            }
        }
        let _ = app.emit("ws-state", "disconnected");
        publish(&svc, MachineEvent::WsState(WsState::Disconnected));

        // A live job whose socket just dropped is now interrupted: freeze it at
        // the last acknowledged line (firmware buffer state is unknown).
        if let Some(j) = job.as_mut() {
            if j.active() {
                j.invalidate_inflight();
                streaming::persist(&app, j, "interrupted");
                streaming::emit_progress(&app, &svc, &job, Some("interrupted"));
            }
        }

        if !running.load(Ordering::SeqCst) {
            break;
        }
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
    let _ = app.emit("ws-state", "disconnected");
    publish(&svc, MachineEvent::WsState(WsState::Disconnected));
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

/// Whether a job is currently occupying the connection (blocking manual input
/// and the telemetry polls).
fn job_active(job: &Option<Job>) -> bool {
    job.as_ref().is_some_and(|j| j.active())
}

/// True when the machine is executing or holding a job we do NOT own (an SD-card
/// run): FluidNC is cutting or holding gcode (`Run`/`Hold`/`Cycle`/`Door`) while
/// the Maslow calibration state is stable (not a belt operation). We must not
/// inject `$MINFO`/`$GSTATE` line commands then. The firmware's update watchdog
/// is only 100 ms (`UPDATE_WATCHDOG_MS`): if the protocol loop stalls past it
/// (e.g. a feed-hold plus extra channel traffic) the firmware latches into a
/// power-cycle-only emergency stop. The embedded UI likewise stays quiet during
/// a job. A Maslow belt op (Retract/Extend/calibrate) keeps the polls so the UI
/// still tracks its state, since that is the calibration flow the firmware
/// expects to be polled.
fn machine_running_unowned_job(ctx: &SocketCtx) -> bool {
    let maslow_op = ctx.tracker.current().is_some_and(|s| s.is_busy());
    matches!(ctx.fluidnc_state.as_str(), "Run" | "Hold" | "Cycle" | "Door") && !maslow_op
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
    upload_active: &Arc<AtomicBool>,
    svc: &Arc<MaslowService>,
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
    publish(svc, MachineEvent::WsState(WsState::Connected));

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
    refresh_policy(app, svc, &mut ctx, job_active(job));

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
                                    handle_incoming(app, svc, &line, &mut ctx, &mut write, job).await?;
                                }
                            }
                            Message::Text(t) => {
                                for raw in t.split('\n') {
                                    let line = raw.trim_end_matches('\r').to_string();
                                    handle_incoming(app, svc, &line, &mut ctx, &mut write, job).await?;
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
                        if job_active(job) {
                            let _ = app.emit("grbl-line",
                                "[blocked] stop the job to send manual commands".to_string());
                            publish(svc, MachineEvent::GrblLine(
                                "[blocked] stop the job to send manual commands".to_string()));
                        } else {
                            let is_action = is_maslow_action(&l);
                            if !l.ends_with('\n') { l.push('\n'); }
                            write.send(Message::Text(l)).await.map_err(|e| e.to_string())?;
                            // This line will return one ok/error; account for it
                            // so it is never miscounted against a job that starts
                            // moments later.
                            ctx.pending.push_back(PendingAck::Untracked);
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
                                ctx.pending.push_back(PendingAck::Untracked);
                                ctx.pending.push_back(PendingAck::Untracked);
                                burst_remaining = BURST_POLLS;
                                maslow_burst.reset();
                            }
                        }
                    }
                    Some(WsCommand::TrackedLine { line, reply }) => {
                        if let Some(reason) = tracked_line_rejected(job_active(job)) {
                            let _ = reply.send(Err(reason.to_string()));
                        } else {
                            let mut l = line;
                            if !l.ends_with('\n') { l.push('\n'); }
                            match write.send(Message::Text(l)).await {
                                Ok(()) => ctx.pending.push_back(PendingAck::Tracked(reply)),
                                Err(e) => {
                                    let _ = reply.send(Err(e.to_string()));
                                }
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
                            streaming::emit_progress(app, svc, job, Some("interrupted"));
                        }
                    }
                    Some(WsCommand::StartJob { lines, path, start_index }) => {
                        *job = Some(Job::new(path, lines, start_index));
                        if let Some(j) = job.as_mut() {
                            streaming::persist(app, j, "running");
                        }
                        pump(&mut write, job).await?;
                        streaming::emit_progress(app, svc, job, None);
                    }
                    Some(WsCommand::PauseJob) => {
                        if let Some(j) = job.as_mut() {
                            j.paused = true;
                            streaming::persist(app, j, "interrupted");
                        }
                        // Feed hold so motion stops promptly, not just queueing.
                        let _ = write.send(Message::Binary(vec![b'!'])).await;
                        streaming::emit_progress(app, svc, job, None);
                    }
                    Some(WsCommand::ResumeJob) => {
                        if let Some(j) = job.as_mut() {
                            j.paused = false;
                        }
                        // Release a possible feed hold before refilling the buffer.
                        let _ = write.send(Message::Binary(vec![b'~'])).await;
                        pump(&mut write, job).await?;
                        streaming::emit_progress(app, svc, job, None);
                    }
                    Some(WsCommand::StopJob) => {
                        // A job stop must actually halt the machine. Clearing the
                        // stream alone leaves the firmware's planner buffer full,
                        // so it keeps cutting the lines already queued. Feed-hold
                        // to decelerate, then a soft reset (0x18) to flush that
                        // buffer and stop. This leaves FluidNC in Alarm; the
                        // operator unlocks/homes before the next job.
                        let _ = write.send(Message::Binary(vec![b'!'])).await;
                        let _ = write.send(Message::Binary(vec![0x18])).await;
                        streaming::clear_saved(app);
                        *job = None;
                        streaming::emit_progress(app, svc, job, None);
                        // The job lock is gone; recompute the action policy.
                        refresh_policy(app, svc, &mut ctx, false);
                    }
                    Some(WsCommand::DumpConfig) => {
                        if job_active(job) {
                            let _ = app.emit("config-dump-error",
                                "stop the job to read the config".to_string());
                            publish(svc, MachineEvent::ConfigDumpError(
                                "stop the job to read the config".to_string()));
                        } else {
                            // Capture the streamed YAML until the terminating ok;
                            // suspend the burst poll so its `ok`s can't truncate it.
                            ctx.capture = Some(Vec::new());
                            ctx.capture_started = Some(Instant::now());
                            burst_remaining = 0;
                            // The capture branch consumes the dump's terminating
                            // ok itself; clear any pending poll/tracked-write acks
                            // so they can't be confused with it (a tracked write
                            // still waiting resolves as an error rather than
                            // hanging until its timeout).
                            clear_pending(&mut ctx.pending, "superseded by a config dump");
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
                        publish(svc, MachineEvent::ConfigDumpError("config dump timed out".to_string()));
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
                    && !job_active(job)
                    && !machine_running_unowned_job(&ctx) {
                    // Short command names ($MINFO/$GSTATE), as the embedded UI uses;
                    // the long `$Maslow/getInfo` form is rejected by the firmware.
                    let _ = write.send(Message::Text("$MINFO\n".to_string())).await;
                    let _ = write.send(Message::Text("$GSTATE\n".to_string())).await;
                    ctx.pending.push_back(PendingAck::Untracked);
                    ctx.pending.push_back(PendingAck::Untracked);
                }
            }
            _ = maslow_burst.tick(), if burst_remaining > 0 => {
                // Fast follow-up after an action so the UI converges in a few
                // hundred ms. Only $GSTATE (cheap, drives the policy); skipped
                // during a job for the same char-counting reason as maslow_tick.
                if !upload_active.load(Ordering::Relaxed)
                    && !job_active(job)
                    && !machine_running_unowned_job(&ctx) {
                    let _ = write.send(Message::Text("$GSTATE\n".to_string())).await;
                    ctx.pending.push_back(PendingAck::Untracked);
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
    svc: &Arc<MaslowService>,
    line: &str,
    ctx: &mut SocketCtx,
    write: &mut WsSink,
    job: &mut Option<Job>,
) -> Result<(), String> {
    if let Some(is_error) = ack_kind(line) {
        // This ack may belong to a poll/manual/tracked command we issued
        // out-of-band, not to a streamed line. Consume it in FIFO order so it
        // cannot drift the job's char-counting cursor.
        let consumed_out_of_band = resolve_pending_ack(&mut ctx.pending, is_error, line);
        if !consumed_out_of_band && job_active(job) {
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
                streaming::emit_progress(app, svc, job, None);
                streaming::clear_saved(app);
                *job = None;
                // Job finished: manual actions become available again.
                refresh_policy(app, svc, ctx, false);
            } else {
                streaming::emit_progress(app, svc, job, None);
            }
        }
    }

    let job_active = job_active(job);
    route_line(app, svc, line, ctx, job_active);
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

/// One outstanding non-job command awaiting its firmware `ok`/`error`, kept in
/// the FIFO queue described on `SocketCtx::pending`.
enum PendingAck {
    /// A poll or fire-and-forget manual line: its reply is simply swallowed.
    Untracked,
    /// A `ws_write_setting`/`ws_save_config` call awaiting the real result.
    Tracked(oneshot::Sender<Result<(), String>>),
}

/// Resolve the oldest pending expectation against an incoming ok/error line, in
/// strict FIFO order. Returns `true` if `line` was consumed this way, meaning
/// the job-ack path must not also treat it as a streamed-line ack; `false`
/// means nothing was pending (fall through to job-ack handling, if any).
fn resolve_pending_ack(pending: &mut VecDeque<PendingAck>, is_error: bool, line: &str) -> bool {
    match pending.pop_front() {
        Some(PendingAck::Untracked) => true,
        Some(PendingAck::Tracked(reply)) => {
            let result = if is_error {
                Err(line.trim().to_string())
            } else {
                Ok(())
            };
            let _ = reply.send(result);
            true
        }
        None => false,
    }
}

/// Drop every outstanding expectation, resolving any tracked write with
/// `reason` so its caller does not hang until the timeout. Used when a `$CD`
/// dump is about to start: its own terminating ok/error must not be confused
/// with an earlier command's, and a write that raced it cannot be trusted to
/// still line up afterwards.
fn clear_pending(pending: &mut VecDeque<PendingAck>, reason: &str) {
    for p in pending.drain(..) {
        if let PendingAck::Tracked(reply) = p {
            let _ = reply.send(Err(reason.to_string()));
        }
    }
}

/// Whether a `TrackedLine` command must be rejected without ever touching the
/// socket: mirrors the same rule as `WsCommand::Line` (a job owns the
/// character-counted stream while it runs, so no other command may interleave
/// with it).
fn tracked_line_rejected(job_active: bool) -> Option<&'static str> {
    if job_active {
        Some("a job is streaming")
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
    /// `ok`/`error` responses still expected from commands we issued that are
    /// NOT job lines (the `$MINFO`/`$GSTATE` polls, the burst poll, any
    /// manual/action line, and tracked writes), in strict FIFO order (the
    /// firmware always answers in the order it received the lines). The job
    /// ack handler swallows exactly these so a stray poll ack landing right as
    /// a job starts can never be miscounted as a streamed-line ack and drift
    /// the char-counting cursor. A `Tracked` entry additionally carries the
    /// oneshot responder for a `ws_write_setting`/`ws_save_config` call.
    pending: VecDeque<PendingAck>,
    /// Set once `maslow-cal-complete` has been emitted for the calibration run
    /// currently ending, so the "Find Anchors complete" log line (which always
    /// follows the state-transition report on the wire) does not re-fire it.
    /// Cleared as soon as any other state observation lands.
    cal_complete_emitted: bool,
}

#[derive(serde::Serialize, Clone)]
pub(crate) struct Discord {
    /// "straggler" = suppressed, "accepted" = machine prevailed (state updated).
    kind: &'static str,
    from: i32,
    to: i32,
    from_label: String,
    to_label: String,
}

fn emit_discord(
    app: &AppHandle,
    svc: &Arc<MaslowService>,
    kind: &'static str,
    from: maslow::CalState,
    to: maslow::CalState,
) {
    let _ = app.emit(
        "maslow-discord",
        Discord {
            kind,
            from: from.code(),
            to: to.code(),
            from_label: from.label().to_string(),
            to_label: to.label().to_string(),
        },
    );
    publish(
        svc,
        MachineEvent::Discord(Discord {
            kind,
            from: from.code(),
            to: to.code(),
            from_label: from.label().to_string(),
            to_label: to.label().to_string(),
        }),
    );
}

/// Emit `maslow-cal-complete` once per calibration run, whichever path (the
/// state transition or the corroborating log line) gets there first.
fn emit_cal_complete(app: &AppHandle, svc: &Arc<MaslowService>, ctx: &mut SocketCtx) {
    if ctx.cal_complete_emitted {
        return;
    }
    ctx.cal_complete_emitted = true;
    let _ = app.emit("maslow-cal-complete", ());
    publish(svc, MachineEvent::CalComplete);
}

/// Recompute the unified action policy and emit it only when it changed.
fn refresh_policy(app: &AppHandle, svc: &Arc<MaslowService>, ctx: &mut SocketCtx, job_active: bool) {
    let p = maslow::action_policy(&ctx.fluidnc_state, ctx.tracker.current(), job_active);
    if ctx.last_policy.as_ref() != Some(&p) {
        ctx.last_policy = Some(p.clone());
        publish(svc, MachineEvent::ActionPolicy(p.clone()));
        let _ = app.emit("action-policy", p);
    }
}

/// Classify and emit a single complete line, updating reconciliation state.
fn route_line(app: &AppHandle, svc: &Arc<MaslowService>, line: &str, ctx: &mut SocketCtx, job_active: bool) {
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
            finalize_config_dump(app, svc, &yaml);
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
        publish(svc, MachineEvent::MachineStatus(status.clone()));
        if changed {
            refresh_policy(app, svc, ctx, job_active);
        }
        return;
    }
    if let Some(info) = maslow::parse_minfo(line) {
        let _ = app.emit("maslow-info", &info);
        publish(svc, MachineEvent::MaslowInfo(info.clone()));
        return;
    }
    if let Some(state) = maslow::parse_state(line) {
        let state = maslow::CalState::from_code(state);
        match ctx.tracker.observe(state, STATE_DEBOUNCE_MS) {
            maslow::Observation::First(s) => {
                ctx.cal_complete_emitted = false;
                let _ = app.emit("maslow-state", maslow::policy_for(s));
                publish(svc, MachineEvent::MaslowState(maslow::policy_for(s)));
                refresh_policy(app, svc, ctx, job_active);
            }
            maslow::Observation::Valid { from, to } => {
                let _ = app.emit("maslow-state", maslow::policy_for(to));
                publish(svc, MachineEvent::MaslowState(maslow::policy_for(to)));
                refresh_policy(app, svc, ctx, job_active);
                if maslow::is_calibration_completion(from, to) {
                    emit_cal_complete(app, svc, ctx);
                } else {
                    // Any other transition means we've moved on from the last
                    // completed (or completing) run, so a future 6->7/9->7 can
                    // fire again.
                    ctx.cal_complete_emitted = false;
                }
            }
            maslow::Observation::Unchanged => {}
            maslow::Observation::Straggler { from, to } => {
                emit_discord(app, svc, "straggler", from, to);
            }
            maslow::Observation::Discord { from, to } => {
                ctx.cal_complete_emitted = false;
                emit_discord(app, svc, "accepted", from, to);
                let _ = app.emit("maslow-state", maslow::policy_for(to));
                publish(svc, MachineEvent::MaslowState(maslow::policy_for(to)));
                refresh_policy(app, svc, ctx, job_active);
            }
        }
        return;
    }
    if let Some(wp) = maslow::parse_waypoint(line) {
        publish(svc, MachineEvent::Waypoint(wp.clone()));
        let _ = app.emit("maslow-waypoint", wp);
        return;
    }
    if let Some(measurements) = calibration::parse_clbm(line) {
        // Raw belt measurements the firmware logs before each recompute — the
        // input the client-side solver re-solves from.
        publish(svc, MachineEvent::CalMeasurements(measurements.clone()));
        let _ = app.emit("cal-measurements", measurements);
        return;
    }
    if let Some(fit) = calibration::parse_fit_line(line) {
        publish(svc, MachineEvent::CalFirmwareFit(fit.clone()));
        let _ = app.emit("cal-firmware-fit", fit);
        // fall through so the fit summary also appears in the console
    }
    if let Some(anchors) = calibration::parse_recompute_line(line) {
        publish(svc, MachineEvent::CalFirmwareAnchors(anchors));
        let _ = app.emit("cal-firmware-anchors", anchors);
        // fall through so the result also appears in the console
    }
    if line.contains("Find Anchors complete") {
        // Belt-and-suspenders: the state-transition path above is authoritative
        // and already emits on 6->7 / 9->7. This log line follows that state
        // report on the wire (requestStateChange prints the new state, then
        // Calibration.cpp logs this message), so skip it if already handled to
        // avoid double-firing the toast; only emit here as a fallback if the
        // transition was, for whatever reason, not observed.
        emit_cal_complete(app, svc, ctx);
        // fall through so the message also appears in the console
    }
    if is_control(line) {
        // CURRENT_ID/ACTIVE_ID are sent once, right after a WS connects (see
        // WSChannels::handleEvent's WStype_CONNECTED case in WSChannel.cpp), and only to
        // that connecting channel: ACTIVE_ID is just wsChannel->id(), which is the same
        // client's own number, so it can never disagree with the CURRENT_ID we were just
        // given. There is no live signal here about a second client connecting; if the
        // firmware ever starts broadcasting a real "another client took over" message,
        // this is where a UI notification would be wired up.
        let _ = app.emit("ws-control", line.to_string());
        publish(svc, MachineEvent::WsControl(line.to_string()));
        return;
    }
    if is_console_chatter(line) {
        return;
    }
    let _ = app.emit("grbl-line", line.to_string());
    publish(svc, MachineEvent::GrblLine(line.to_string()));
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
fn finalize_config_dump(app: &AppHandle, svc: &Arc<MaslowService>, yaml: &str) {
    let entries = crate::fluidnc::flatten_config(yaml);
    if entries.is_empty() {
        let _ = app.emit("config-dump-error", "empty or unparseable config dump".to_string());
        publish(svc, MachineEvent::ConfigDumpError("empty or unparseable config dump".to_string()));
        return;
    }
    let synthetic: String = entries
        .iter()
        .map(|e| format!("{}={}\n", e.path, e.value))
        .collect();
    let _ = app.emit("config-dump", &entries);
    publish(svc, MachineEvent::ConfigDump(entries.clone()));
    if let Some(anchors) = maslow::parse_anchors(&synthetic) {
        let _ = app.emit("maslow-anchors", &anchors);
        publish(svc, MachineEvent::Anchors(anchors.clone()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracked_write_ok_resolves_ok() {
        let mut pending = VecDeque::new();
        let (reply, mut rx) = oneshot::channel();
        pending.push_back(PendingAck::Tracked(reply));

        assert!(resolve_pending_ack(&mut pending, false, "ok"));
        assert!(pending.is_empty());
        match rx.try_recv() {
            Ok(result) => assert_eq!(result, Ok(())),
            Err(e) => panic!("expected a reply, got {e:?}"),
        }
    }

    #[test]
    fn tracked_write_error_surfaces_firmware_text() {
        let mut pending = VecDeque::new();
        let (reply, mut rx) = oneshot::channel();
        pending.push_back(PendingAck::Tracked(reply));

        assert!(resolve_pending_ack(&mut pending, true, "error:3"));
        match rx.try_recv() {
            Ok(result) => assert_eq!(result, Err("error:3".to_string())),
            Err(e) => panic!("expected a reply, got {e:?}"),
        }
    }

    #[test]
    fn tracked_write_rejected_while_job_active() {
        // Mirrors the `WsCommand::Line` guard: a job owns the socket, so a
        // tracked write must be refused before it ever reaches the socket.
        assert_eq!(tracked_line_rejected(true), Some("a job is streaming"));
        assert_eq!(tracked_line_rejected(false), None);
    }

    #[test]
    fn fifo_order_is_preserved_across_mixed_entries() {
        // An untracked poll ack ahead of a tracked write must be consumed
        // first, leaving the tracked entry to match the next ok/error.
        let mut pending = VecDeque::new();
        pending.push_back(PendingAck::Untracked);
        let (reply, mut rx) = oneshot::channel();
        pending.push_back(PendingAck::Tracked(reply));

        assert!(resolve_pending_ack(&mut pending, false, "ok"));
        assert_eq!(pending.len(), 1);
        assert!(resolve_pending_ack(&mut pending, true, "error:9"));
        match rx.try_recv() {
            Ok(result) => assert_eq!(result, Err("error:9".to_string())),
            Err(e) => panic!("expected a reply, got {e:?}"),
        }
    }

    #[test]
    fn no_pending_entry_falls_through() {
        let mut pending = VecDeque::new();
        assert!(!resolve_pending_ack(&mut pending, false, "ok"));
    }

    #[test]
    fn clear_pending_resolves_tracked_entries_with_reason() {
        let mut pending = VecDeque::new();
        pending.push_back(PendingAck::Untracked);
        let (reply, mut rx) = oneshot::channel();
        pending.push_back(PendingAck::Tracked(reply));

        clear_pending(&mut pending, "superseded by a config dump");
        assert!(pending.is_empty());
        match rx.try_recv() {
            Ok(result) => assert_eq!(result, Err("superseded by a config dump".to_string())),
            Err(e) => panic!("expected a reply, got {e:?}"),
        }
    }
}
