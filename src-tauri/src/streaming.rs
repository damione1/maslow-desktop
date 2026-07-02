// G-code job streaming with the GRBL character-counting protocol and
// crash-resumable progress.
//
// A `Job` holds the parsed lines of a file plus two cursors:
//   - `sent`  : index of the next line to push onto the wire
//   - `acked` : number of lines the firmware confirmed with `ok`/`error`
// `acked` is the only cursor that matters for resuming: it is the exact point
// up to which the machine has executed. We persist it to disk so the job can be
// resumed after an unexpected disconnect or an app restart.
//
// Ownership lives in the connection supervisor (`connection_loop`), so the job
// survives the automatic WebSocket reconnects; the on-disk copy survives a full
// process restart.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};

/// GRBL character-counting RX budget. The firmware's actual WS channel buffer
/// is 256 bytes (`WSChannel::RXBUFFERSIZE`), but we deliberately budget for
/// half of that: it halves the worst-case queue depth in flight, at the cost
/// of a bit of throughput. Keep 127 when re-tuning; start from 256, not 128.
const RX_BUFFER: usize = 127;
/// Persist to disk at most every N acked lines (plus on every state change).
const PERSIST_EVERY: usize = 25;

/// Live job state pushed to the frontend.
#[derive(Serialize, Clone)]
pub struct Progress {
    /// "running" | "paused" | "interrupted" | "done" | "error" | "idle"
    pub state: String,
    pub path: String,
    pub name: String,
    pub sent: usize,
    pub acked: usize,
    pub total: usize,
    pub errors: usize,
}

/// On-disk record so a job survives a process restart.
#[derive(Serialize, Deserialize, Clone)]
pub struct SavedJob {
    pub path: String,
    pub name: String,
    pub total: usize,
    pub acked: usize,
    pub state: String,
    pub updated_at: u64,
}

pub struct Job {
    pub path: String,
    pub name: String,
    pub lines: Vec<String>,
    pub total: usize,
    pub sent: usize,
    pub acked: usize,
    pub errors: usize,
    pub paused: bool,
    pub done: bool,
    /// Byte length (line + newline) of each sent-but-unacked line.
    inflight: VecDeque<usize>,
    inflight_bytes: usize,
    last_persist_acked: usize,
    /// A modal-state preamble to replay before the next streamed line, set when
    /// resuming mid-file (the leading G20/G21/G90/G54/F block sits below `acked`
    /// and the firmware may have lost its modal state across the interruption).
    /// Sent once, ahead of the file lines; its ack is swallowed so it does not
    /// advance `acked`.
    preamble: Option<String>,
    /// Whether the preamble line is currently sent-but-unacked.
    preamble_inflight: bool,
}

impl Job {
    pub fn new(path: String, lines: Vec<String>, start_index: usize) -> Self {
        let total = lines.len();
        let start = start_index.min(total);
        let name = file_name(&path);
        // Resuming mid-file: replay the modal state that was active at the resume
        // point so a reboot/units/WCS loss can't scale or offset the cut.
        let preamble = if start > 0 {
            modal_preamble(&lines[..start])
        } else {
            None
        };
        Job {
            path,
            name,
            lines,
            total,
            sent: start,
            acked: start,
            errors: 0,
            paused: false,
            done: false,
            inflight: VecDeque::new(),
            inflight_bytes: 0,
            last_persist_acked: start,
            preamble,
            preamble_inflight: false,
        }
    }

    /// Is the job still occupying the connection (blocking manual input)?
    pub fn active(&self) -> bool {
        !self.done
    }

    fn state_label(&self) -> &'static str {
        if self.done {
            if self.errors > 0 { "error" } else { "done" }
        } else if self.paused {
            "paused"
        } else {
            "running"
        }
    }

    pub fn progress(&self, state_override: Option<&str>) -> Progress {
        Progress {
            state: state_override.unwrap_or(self.state_label()).to_string(),
            path: self.path.clone(),
            name: self.name.clone(),
            sent: self.sent,
            acked: self.acked,
            total: self.total,
            errors: self.errors,
        }
    }

    /// Next line to send, if the firmware buffer has room for it.
    /// Returns the line *with* its trailing newline.
    pub fn next_line(&mut self) -> Option<String> {
        if self.paused || self.done {
            return None;
        }
        // Replay the resume preamble first (once), before any file line. Its byte
        // length counts against the RX budget like any other line; its ack is
        // swallowed in `ack` so it never advances `acked`.
        if !self.preamble_inflight {
            if let Some(pre) = self.preamble.take() {
                let len = pre.len() + 1;
                if self.inflight_bytes + len > RX_BUFFER && !self.inflight.is_empty() {
                    // No room yet — put it back and wait for buffer space.
                    self.preamble = Some(pre);
                    return None;
                }
                self.inflight.push_back(len);
                self.inflight_bytes += len;
                self.preamble_inflight = true;
                return Some(format!("{pre}\n"));
            }
        }
        if self.sent >= self.total {
            return None;
        }
        let len = self.lines[self.sent].len() + 1;
        // Respect the RX budget, but always allow a single oversized line through
        // when nothing else is in flight (otherwise we would deadlock).
        if self.inflight_bytes + len > RX_BUFFER && !self.inflight.is_empty() {
            return None;
        }
        let out = format!("{}\n", self.lines[self.sent]);
        self.inflight.push_back(len);
        self.inflight_bytes += len;
        self.sent += 1;
        Some(out)
    }

    /// Account for one `ok`/`error` acknowledgement. Returns true if the job
    /// just completed.
    pub fn ack(&mut self, is_error: bool) -> bool {
        if self.done {
            return false;
        }
        if let Some(len) = self.inflight.pop_front() {
            self.inflight_bytes -= len;
            // The first inflight entry after a resume is the modal preamble, not
            // a file line: free its bytes but do NOT advance `acked`.
            if self.preamble_inflight {
                self.preamble_inflight = false;
                return false;
            }
            self.acked += 1;
            if is_error {
                self.errors += 1;
            }
        }
        if self.acked >= self.total && self.inflight.is_empty() {
            self.done = true;
            return true;
        }
        false
    }

    /// A hard reset (Ctrl-X) flushed the firmware buffer; our in-flight count is
    /// no longer meaningful. Freeze the job at the last confirmed line.
    pub fn invalidate_inflight(&mut self) {
        self.inflight.clear();
        self.inflight_bytes = 0;
        self.sent = self.acked;
        self.paused = true;
        self.preamble_inflight = false;
        // The connection broke (or a soft reset flushed the firmware): the
        // machine may have lost its modal state. Re-arm a preamble so the next
        // resume replays the units/distance/WCS/feed active at the resume point.
        if self.acked > 0 {
            self.preamble = modal_preamble(&self.lines[..self.acked]);
        }
    }

    fn should_persist(&self) -> bool {
        self.acked >= self.last_persist_acked + PERSIST_EVERY
    }
}

/// Emit a progress event for the current job (or an idle event when None).
pub fn emit_progress(app: &AppHandle, job: &Option<Job>, state_override: Option<&str>) {
    let p = match job {
        Some(j) => j.progress(state_override),
        None => Progress {
            state: "idle".into(),
            path: String::new(),
            name: String::new(),
            sent: 0,
            acked: 0,
            total: 0,
            errors: 0,
        },
    };
    let _ = app.emit("stream-progress", p);
}

fn saved_path(app: &AppHandle) -> Option<PathBuf> {
    let dir = app.path().app_data_dir().ok()?;
    Some(dir.join("current_job.json"))
}

/// Persist the job's resumable cursor to disk.
pub fn persist(app: &AppHandle, job: &mut Job, state: &str) {
    job.last_persist_acked = job.acked;
    let Some(path) = saved_path(app) else { return };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let rec = SavedJob {
        path: job.path.clone(),
        name: job.name.clone(),
        total: job.total,
        acked: job.acked,
        state: state.to_string(),
        updated_at: now,
    };
    if let Ok(json) = serde_json::to_string_pretty(&rec) {
        let _ = std::fs::write(path, json);
    }
}

/// Persist after an acknowledgement, throttled.
pub fn maybe_persist(app: &AppHandle, job: &mut Job) {
    if job.should_persist() {
        persist(app, job, "running");
    }
}

/// Remove the persisted record (job fully done or discarded).
pub fn clear_saved(app: &AppHandle) {
    if let Some(path) = saved_path(app) {
        let _ = std::fs::remove_file(path);
    }
}

/// Read the persisted job, if any, and only when it is genuinely resumable
/// (interrupted with remaining lines). Returns None for done/empty jobs.
pub fn read_saved(app: &AppHandle) -> Option<SavedJob> {
    let path = saved_path(app)?;
    let data = std::fs::read_to_string(path).ok()?;
    let rec: SavedJob = serde_json::from_str(&data).ok()?;
    if rec.state == "done" || rec.acked >= rec.total {
        return None;
    }
    Some(rec)
}

/// Load a G-code file and split it into streamable lines.
/// Blank lines and full-line comments are dropped so that the `ok` count maps
/// one-to-one onto our `lines` vector.
pub fn load_gcode(path: &str) -> Result<Vec<String>, String> {
    let raw = std::fs::read_to_string(path).map_err(|e| format!("read {path}: {e}"))?;
    Ok(raw.lines().filter_map(clean_line).collect())
}

/// Strip a trailing `;` comment, trim, and drop empty / pure-comment lines.
/// Dropping pure comments keeps the `ok` count one-to-one with our lines:
/// FluidNC does not always emit an `ok` for a line that carries no command.
pub(crate) fn clean_line(line: &str) -> Option<String> {
    let mut s = line;
    if let Some(idx) = s.find(';') {
        s = &s[..idx];
    }
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    // A line that is nothing but a parenthesised comment, e.g. "(facing pass)".
    if s.starts_with('(') && s.ends_with(')') {
        return None;
    }
    Some(s.to_string())
}

fn file_name(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or(path).to_string()
}

/// Build a modal-state preamble from the lines preceding a resume point, so the
/// resumed stream re-establishes the units / distance mode / plane / work
/// coordinate system / feed that were in effect there. Only modal words that
/// actually appeared in the skipped region are replayed (we never invent a
/// default the file did not set). Returns None when there is nothing to replay.
pub(crate) fn modal_preamble(lines: &[String]) -> Option<String> {
    let mut units: Option<&str> = None; // G20 | G21
    let mut distance: Option<&str> = None; // G90 | G91
    let mut plane: Option<&str> = None; // G17 | G18 | G19
    let mut wcs: Option<String> = None; // G54 .. G59
    let mut feed: Option<String> = None; // F<n>

    for line in lines {
        for (letter, num) in modal_words(line) {
            match letter {
                'G' => match num.as_str() {
                    "20" => units = Some("G20"),
                    "21" => units = Some("G21"),
                    "90" => distance = Some("G90"),
                    "91" => distance = Some("G91"),
                    "17" => plane = Some("G17"),
                    "18" => plane = Some("G18"),
                    "19" => plane = Some("G19"),
                    "54" | "55" | "56" | "57" | "58" | "59" => wcs = Some(format!("G{num}")),
                    _ => {}
                },
                'F' if !num.is_empty() => feed = Some(format!("F{num}")),
                _ => {}
            }
        }
    }

    let mut parts: Vec<String> = Vec::new();
    if let Some(d) = distance {
        parts.push(d.to_string());
    }
    if let Some(u) = units {
        parts.push(u.to_string());
    }
    if let Some(p) = plane {
        parts.push(p.to_string());
    }
    if let Some(w) = wcs {
        parts.push(w);
    }
    if let Some(f) = feed {
        parts.push(f);
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" "))
    }
}

/// Tokenise a G-code line into (letter, number) words, tolerating concatenated
/// forms like `G21G90`. The number keeps digits, sign and decimal point.
fn modal_words(line: &str) -> Vec<(char, String)> {
    let chars: Vec<char> = line.to_ascii_uppercase().chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_ascii_alphabetic() {
            let letter = chars[i];
            i += 1;
            let start = i;
            while i < chars.len()
                && (chars[i].is_ascii_digit() || matches!(chars[i], '.' | '-' | '+'))
            {
                i += 1;
            }
            out.push((letter, chars[start..i].iter().collect()));
        } else {
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_filters_lines() {
        let raw = "G21\n; comment\n(only comment)\n\nG0 X10 ; inline\nM3";
        let tmp = std::env::temp_dir().join("maslow_test.gcode");
        std::fs::write(&tmp, raw).unwrap();
        let lines = load_gcode(tmp.to_str().unwrap()).unwrap();
        assert_eq!(lines, vec!["G21", "G0 X10", "M3"]);
    }

    #[test]
    fn char_counting_respects_buffer() {
        let lines: Vec<String> = (0..10).map(|i| format!("G1 X{i}")).collect();
        let mut job = Job::new("/x.nc".into(), lines, 0);
        let mut sent = 0;
        while job.next_line().is_some() {
            sent += 1;
        }
        // Should fill the buffer, not the whole file in one go.
        assert!(sent > 0 && sent <= 10);
        assert!(job.inflight_bytes <= RX_BUFFER);
    }

    #[test]
    fn ack_advances_and_completes() {
        let lines = vec!["G1 X1".to_string(), "G1 X2".to_string()];
        let mut job = Job::new("/x.nc".into(), lines, 0);
        assert!(job.next_line().is_some());
        assert!(job.next_line().is_some());
        assert!(job.next_line().is_none());
        assert!(!job.ack(false));
        assert!(job.ack(false)); // second ack completes
        assert!(job.done);
        assert_eq!(job.acked, 2);
    }

    #[test]
    fn resume_starts_at_index() {
        let lines: Vec<String> = (0..5).map(|i| format!("L{i}")).collect();
        let job = Job::new("/x.nc".into(), lines, 3);
        assert_eq!(job.sent, 3);
        assert_eq!(job.acked, 3);
    }

    #[test]
    fn modal_preamble_replays_active_modals() {
        let lines: Vec<String> = vec![
            "G21".into(),
            "G90".into(),
            "G54".into(),
            "F800".into(),
            "G1 X10 Y10".into(),
            "G20".into(), // a later units change, must win
            "G1 X20".into(),
        ];
        // Resume after all 7 lines: the active units are now G20.
        let pre = modal_preamble(&lines).unwrap();
        assert!(pre.contains("G90"));
        assert!(pre.contains("G20") && !pre.contains("G21"));
        assert!(pre.contains("G54"));
        assert!(pre.contains("F800"));
    }

    #[test]
    fn modal_preamble_handles_concatenated_and_empty() {
        assert_eq!(modal_preamble(&["G21G90".into()]).as_deref(), Some("G90 G21"));
        // No modal words → nothing to replay.
        assert!(modal_preamble(&["G1 X1 Y2".into(), "M3".into()]).is_none());
    }

    #[test]
    fn resume_preamble_is_sent_first_and_not_counted() {
        let lines: Vec<String> = vec![
            "G21".into(),
            "G90".into(),
            "G1 X1".into(),
            "G1 X2".into(),
            "G1 X3".into(),
        ];
        let mut job = Job::new("/x.nc".into(), lines, 3);
        // First line out is the preamble, not a file line.
        let first = job.next_line().unwrap();
        assert!(first.starts_with("G90 G21"));
        assert_eq!(job.acked, 3, "preamble must not advance acked yet");
        // Acking the preamble frees its bytes but keeps acked at the resume point.
        assert!(!job.ack(false));
        assert_eq!(job.acked, 3);
        // Then the two remaining real lines stream and complete normally.
        assert!(job.next_line().is_some());
        assert!(job.next_line().is_some());
        assert!(job.next_line().is_none());
        assert!(!job.ack(false));
        assert!(job.ack(false));
        assert!(job.done);
        assert_eq!(job.acked, 5);
    }
}
