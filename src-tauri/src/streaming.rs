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

/// GRBL serial RX buffer budget for character counting (128 - 1).
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
}

impl Job {
    pub fn new(path: String, lines: Vec<String>, start_index: usize) -> Self {
        let total = lines.len();
        let start = start_index.min(total);
        let name = file_name(&path);
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
        if self.paused || self.done || self.sent >= self.total {
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
}
