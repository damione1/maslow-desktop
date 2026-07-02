// Maslow-specific telemetry: the MINFO JSON blob and the calibration state
// machine. Mirrors firmware Maslow.cpp::getInfo() and MaslowEnums.h.
//
// MINFO line (from `$Maslow/getInfo`):
//   MINFO: { "homed": true, "calibrationInProgress": false, "tl": 1234.5,
//            "tr": .., "br": .., "bl": .., "etl": .., "etr": .., "ebr": ..,
//            "ebl": .., "extended": false }
//
// State (0-9) comes separately from `[MSG:INFO: Current state: N]`
// (also emitted on demand by `$Maslow/gstate`); it is NOT part of MINFO.

use serde::{Deserialize, Serialize};
use std::time::Instant;

#[derive(Serialize, Deserialize, Clone, Default, Debug)]
pub struct MaslowInfo {
    pub homed: bool,
    #[serde(rename = "calibrationInProgress")]
    pub calibration_in_progress: bool,
    /// Belt lengths (mm) for each arm. `None` when the firmware reported a
    /// NaN/Inf reading for that arm (see `sanitize_minfo_json`).
    pub tl: Option<f32>,
    pub tr: Option<f32>,
    pub br: Option<f32>,
    pub bl: Option<f32>,
    /// Belt position errors (mm) for each arm. `None` when the firmware
    /// reported a NaN/Inf reading for that arm.
    pub etl: Option<f32>,
    pub etr: Option<f32>,
    pub ebr: Option<f32>,
    pub ebl: Option<f32>,
    pub extended: bool,
}

/// Replace bare `nan` / `inf` / `-inf` / `+inf` tokens (the firmware's `%g`
/// printf output for a NaN/Inf belt reading, glibc/newlib style) with `null`
/// when they appear as JSON values, so the line parses as valid JSON. Only
/// touches unquoted alphabetic runs outside of string literals; this MINFO
/// blob is a small, fixed, flat JSON shape (no nested objects/arrays), so a
/// hand-rolled scan is enough and avoids pulling in the `regex` crate, which
/// isn't already a dependency of this crate.
fn sanitize_minfo_json(json: &str) -> String {
    let mut out = String::with_capacity(json.len());
    let mut chars = json.char_indices().peekable();
    let mut in_string = false;

    while let Some((i, c)) = chars.next() {
        if in_string {
            out.push(c);
            if c == '\\' {
                // Preserve the escaped character verbatim (e.g. `\"`) without
                // reinterpreting it as a closing quote.
                if let Some(&(_, next)) = chars.peek() {
                    out.push(next);
                    chars.next();
                }
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }

        if c == '"' {
            in_string = true;
            out.push(c);
            continue;
        }

        if c.is_ascii_alphabetic() {
            // Collect the maximal run of ASCII letters starting here. Outside
            // a string, the only such runs this JSON shape can contain are
            // `true`/`false`/`null` or the bad `nan`/`inf` tokens (numbers
            // never contain letters other than a bare exponent `e`, which is
            // a single-character run and never matches).
            let start = i;
            let mut end = i + c.len_utf8();
            while let Some(&(j, nc)) = chars.peek() {
                if nc.is_ascii_alphabetic() {
                    end = j + nc.len_utf8();
                    chars.next();
                } else {
                    break;
                }
            }
            let word = &json[start..end];
            if word.eq_ignore_ascii_case("nan") || word.eq_ignore_ascii_case("inf") {
                // Absorb a sign that belongs to this token (e.g. `-inf`).
                if out.ends_with('-') || out.ends_with('+') {
                    out.pop();
                }
                out.push_str("null");
            } else {
                out.push_str(word);
            }
            continue;
        }

        out.push(c);
    }

    out
}

/// Parse a `MINFO: { ... }` telemetry line. Returns None if it is not a MINFO
/// line or the JSON is malformed. NaN/Inf belt readings (rendered as bare
/// `nan`/`inf`/`-inf` by the firmware's `%g` formatting) are sanitized to
/// `null` first, so a single bad reading degrades that field to `None`
/// instead of failing the whole parse.
pub fn parse_minfo(line: &str) -> Option<MaslowInfo> {
    let json = line.trim().strip_prefix("MINFO:")?.trim();
    let sanitized = sanitize_minfo_json(json);
    serde_json::from_str(&sanitized).ok()
}

/// Parse a `[MSG:INFO: Current state: N]` line into the state number.
pub fn parse_state(line: &str) -> Option<i32> {
    let idx = line.find("Current state:")?;
    let rest = &line[idx + "Current state:".len()..];
    let digits: String = rest
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

/// Maslow calibration state (mirrors the firmware `MaslowEnums.h`). The firmware
/// owns this and reports it as an integer over the wire; we keep a typed enum
/// internally so every match is exhaustive. Unrecognised codes map to `Unknown`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum CalState {
    Unknown = 0,
    Retracting = 1,
    Retracted = 2,
    Extending = 3,
    ExtendedOut = 4,
    TakingSlack = 5,
    CalibrationInProgress = 6,
    ReadyToCut = 7,
    ReleaseTension = 8,
    CalibrationComputing = 9,
}

impl CalState {
    /// Map a firmware-reported integer to a state; anything unexpected → Unknown.
    pub fn from_code(code: i32) -> Self {
        use CalState::*;
        match code {
            1 => Retracting,
            2 => Retracted,
            3 => Extending,
            4 => ExtendedOut,
            5 => TakingSlack,
            6 => CalibrationInProgress,
            7 => ReadyToCut,
            8 => ReleaseTension,
            9 => CalibrationComputing,
            _ => Unknown,
        }
    }

    /// The numeric code, as the frontend and firmware use it.
    pub fn code(self) -> i32 {
        self as i32
    }

    pub fn label(self) -> &'static str {
        use CalState::*;
        match self {
            Unknown => "Unknown",
            Retracting => "Retracting",
            Retracted => "Retracted",
            Extending => "Extending",
            ExtendedOut => "Extended",
            TakingSlack => "Taking Slack",
            CalibrationInProgress => "Calibrating",
            ReadyToCut => "Ready to Cut",
            ReleaseTension => "Releasing Tension",
            CalibrationComputing => "Computing",
        }
    }

    /// True while the machine is actively performing a transitional operation;
    /// only Retract / Stop / E-Stop are offered then.
    ///
    /// Treating `CalibrationComputing` (state 9) as busy/Retract-only is a deliberate
    /// simplification, not a port of the firmware's raw transition graph for that state.
    /// On v1.22+ firmware never enters state 9 in normal operation (anchor recompute moved
    /// on-device). On v1.21, the firmware does enter it and waits there for the client to
    /// send `$ACKCAL` with recomputed anchors; this app intentionally does not implement
    /// that handshake, so it has nothing useful to do in state 9 beyond offering recovery.
    /// The frontend gates the Calibrate action itself by firmware version (see
    /// `supportsFullCalibration` in `src/lib/stores/firmware.ts`), so state 9 should never
    /// be reached through this app at all; this is a fallback for the case it somehow is.
    pub fn is_busy(self) -> bool {
        use CalState::*;
        matches!(
            self,
            Retracting | Extending | TakingSlack | CalibrationInProgress | ReleaseTension | CalibrationComputing
        )
    }
}

/// The action policy the UI applies for a given calibration state: which user
/// actions are offered and whether the machine is mid-operation.
#[derive(Serialize, Clone, Debug)]
pub struct StatePolicy {
    pub code: i32,
    pub label: String,
    /// True while the machine is actively performing an operation (transitional
    /// state). Only Stop / E-Stop are offered then.
    pub busy: bool,
    /// Allowed user action ids: retract, extend, takeSlack, calibrate, comply,
    /// stop, estop. Used only inside Rust (the frontend gates on the reconciled
    /// `ActionPolicy` booleans), so it is kept off the wire.
    #[serde(skip)]
    pub allowed: Vec<String>,
}

/// Single source of truth for per-state allowed actions, derived from
/// `Calibration::requestStateChange()` in the firmware.
///
/// Retract / Stop / E-Stop are ALWAYS allowed, mirroring the firmware exactly:
/// `requestStateChange` accepts RETRACTING from *any* state, and `$STOP`/`$ESTOP`
/// are unconditional. This is critical for recovery: `maslow_stop` ($STOP) stops
/// the motors and sets FluidNC to Idle but NEVER resets the Maslow FSM, so a Stop
/// pressed mid-EXTENDING(3) leaves `currentState` frozen at EXTENDING forever.
/// From there the firmware refuses Extend (needs RETRACTED/EXTENDEDOUT) but still
/// accepts Retract — so Retract is the ONLY way out. Gating it behind `busy`
/// (as we used to) disabled the single recovery command and left the user stuck.
///
/// The remaining actions (extend / takeSlack / calibrate / comply) genuinely
/// require a specific stable source state, so they stay gated behind `!busy`.
pub fn policy_for(state: CalState) -> StatePolicy {
    use CalState::*;
    let busy = state.is_busy();

    // Always available, even in busy/transitional states — see doc comment.
    // Retract is the universal escape hatch from a frozen FSM.
    let mut allowed: Vec<String> = vec!["retract".into(), "stop".into(), "estop".into()];

    if !busy {
        match state {
            Retracted => allowed.push("extend".into()),
            ExtendedOut => {
                allowed.push("extend".into());
                allowed.push("takeSlack".into());
                allowed.push("calibrate".into());
                allowed.push("comply".into());
            }
            ReadyToCut => {
                allowed.push("takeSlack".into());
                allowed.push("calibrate".into());
                allowed.push("comply".into());
            }
            Unknown => allowed.push("comply".into()),
            _ => {}
        }
    }

    StatePolicy {
        code: state.code(),
        label: state.label().to_string(),
        busy,
        allowed,
    }
}

// --- Transition validation -------------------------------------------------
//
// The firmware reports its calibration state asynchronously and the reports can
// arrive out of order or lag behind an action we just triggered. We validate
// each reported state against the firmware's own transition graph (the success
// conditions in Calibration::requestStateChange) so we can recognise stale
// "straggler" reports and log genuine discordances.

/// True if `from -> to` is a transition the firmware can actually perform.
/// `from == to` is always valid (an idempotent re-report).
pub fn valid_transition(from: CalState, to: CalState) -> bool {
    use CalState::*;
    if from == to {
        return true;
    }
    match to {
        // Accepted from any state by requestStateChange.
        Unknown | Retracting => true,
        Retracted => from == Retracting,
        Extending => matches!(from, Retracted | ExtendedOut),
        ExtendedOut => matches!(
            from,
            Extending | TakingSlack | ReleaseTension | CalibrationComputing | CalibrationInProgress
        ),
        TakingSlack => matches!(from, ExtendedOut | ReadyToCut),
        CalibrationInProgress => matches!(from, ExtendedOut | ReadyToCut | CalibrationComputing),
        CalibrationComputing => from == CalibrationInProgress,
        ReadyToCut => matches!(from, CalibrationInProgress | CalibrationComputing | TakingSlack),
        ReleaseTension => matches!(from, ReadyToCut | Unknown | ExtendedOut | CalibrationComputing),
    }
}

/// True when a `from -> to` transition represents the completion of a
/// calibration run: entering READY_TO_CUT out of the calibration solve path
/// (CALIBRATION_IN_PROGRESS or CALIBRATION_COMPUTING). A daily apply-tension
/// cycle also ends at READY_TO_CUT but comes from TAKING_SLACK, so it must NOT
/// be reported as a completion here.
pub fn is_calibration_completion(from: CalState, to: CalState) -> bool {
    use CalState::*;
    to == ReadyToCut && matches!(from, CalibrationInProgress | CalibrationComputing)
}

/// Outcome of observing a reported state through the tracker.
#[derive(Debug, PartialEq)]
pub enum Observation {
    /// First state we have ever seen.
    First(CalState),
    /// Same as the current state; nothing to do.
    Unchanged,
    /// A legitimate transition; state updated.
    Valid { from: CalState, to: CalState },
    /// Invalid transition arriving within the debounce window — treated as a
    /// late straggler and ignored (state NOT updated).
    Straggler { from: CalState, to: CalState },
    /// Invalid transition outside the debounce window — the machine's report
    /// prevails, so state IS updated, but the discordance is logged.
    Discord { from: CalState, to: CalState },
}

/// Tracks the authoritative Maslow calibration state. The machine's report
/// always wins, but reports that contradict the transition graph and land just
/// after a change are suppressed as stragglers to avoid UI flicker.
pub struct StateTracker {
    current: Option<CalState>,
    last_change: Instant,
}

impl Default for StateTracker {
    fn default() -> Self {
        Self {
            current: None,
            last_change: Instant::now(),
        }
    }
}

impl StateTracker {
    pub fn current(&self) -> Option<CalState> {
        self.current
    }

    pub fn observe(&mut self, to: CalState, debounce_ms: u64) -> Observation {
        match self.current {
            None => {
                self.current = Some(to);
                self.last_change = Instant::now();
                Observation::First(to)
            }
            Some(cur) if cur == to => Observation::Unchanged,
            Some(cur) => {
                if valid_transition(cur, to) {
                    self.current = Some(to);
                    self.last_change = Instant::now();
                    Observation::Valid { from: cur, to }
                } else if (self.last_change.elapsed().as_millis() as u64) < debounce_ms {
                    Observation::Straggler { from: cur, to }
                } else {
                    self.current = Some(to);
                    self.last_change = Instant::now();
                    Observation::Discord { from: cur, to }
                }
            }
        }
    }
}

// --- Unified action policy -------------------------------------------------
//
// Reconciles BOTH state machines: the base FluidNC state (gates manual motion,
// per the firmware command guards) and the Maslow calibration state (gates the
// belt/calibration actions, per requestStateChange). A streaming job locks out
// everything except realtime controls.

/// Which UI actions are allowed right now. Assumes a live connection; the
/// frontend additionally ANDs each field with the connection state.
#[derive(Serialize, Clone, Debug, Default, PartialEq)]
pub struct ActionPolicy {
    // Manual motion — gated by the FluidNC machine state.
    pub jog: bool,
    pub home: bool,
    pub unlock: bool,
    pub zero: bool,
    pub run: bool,
    // Realtime controls — always available on a live socket.
    pub hold: bool,
    pub resume: bool,
    pub reset: bool,
    // Maslow belt / calibration — gated by the calibration state.
    pub retract: bool,
    pub extend: bool,
    pub take_slack: bool,
    pub calibrate: bool,
    pub comply: bool,
    pub stop: bool,
    pub estop: bool,
}

/// Compute the allowed actions from the FluidNC state string, the Maslow
/// calibration state, and whether a G-code job is streaming.
pub fn action_policy(fluidnc: &str, maslow: Option<CalState>, job_active: bool) -> ActionPolicy {
    let idle = fluidnc == "Idle";
    let alarm = fluidnc == "Alarm";
    let jogging = fluidnc == "Jog";
    // A stable state to *start* a belt/calibration op (these then drive Homing).
    let stable = idle || alarm;

    // Realtime controls, gated on the FluidNC state so the buttons reflect what
    // the command actually does: feed-hold only while something is moving, resume
    // only while held. Reset (the soft-reset kill) is always available on a live
    // link. These are out-of-band bytes, safe even mid-job.
    let mut p = ActionPolicy {
        hold: matches!(fluidnc, "Run" | "Jog" | "Home" | "Homing" | "Cycle"),
        resume: matches!(fluidnc, "Hold" | "Door"),
        reset: true,
        ..Default::default()
    };

    if !job_active {
        p.jog = idle || jogging; // $J= — notIdleOrJog
        p.home = idle || alarm; // $H — notIdleOrAlarm
        p.zero = idle; // G10 line
        // Start streaming ONLY when the belts are tensioned and the machine is
        // idle. The firmware powers the XY belt PID exclusively in READY_TO_CUT
        // (Maslow.cpp); streaming a job in any other state (EXTENDEDOUT, UNKNOWN,
        // mid-calibration) drives the spindle and Z while the XY belts hang slack
        // — an uncontrolled cut. This also blocks a blind resume after a reboot,
        // when the machine comes back in Alarm / a non-cut state.
        p.run = idle && maslow == Some(CalState::ReadyToCut);
        // $X is a *line* command, so it must be blocked while a job streams
        // (it would corrupt char-counting); the firmware accepts it anyState.
        p.unlock = true;

        if let Some(ms) = maslow {
            let pol = policy_for(ms);
            let has = |a: &str| pol.allowed.iter().any(|x| x == a);
            // Retract is NOT gated by `stable`: the firmware accepts RETRACTING
            // from any state, and a $STOP leaves the Maslow FSM frozen (FluidNC
            // back to Idle), so Retract is the recovery action and must stay live
            // even in busy/transitional calibration states. The job lock above
            // still blocks it during a stream (handled by the `!job_active` gate).
            p.retract = has("retract");
            // The rest require a specific stable source state to start.
            p.extend = stable && has("extend");
            p.take_slack = stable && has("takeSlack");
            p.calibrate = stable && has("calibrate");
            p.comply = stable && has("comply");
        }
        // $STOP / $ESTOP are line commands, blocked while a job streams; the
        // in-cut emergency is the realtime Reset.
        p.stop = true;
        p.estop = true;
    }

    p
}

// --- Anchor configuration --------------------------------------------------
//
// The frame anchor coordinates live in the firmware config under
// `kinematics/MaslowKinematics/<key>` (MaslowKinematics.cpp::group) and are
// persisted in maslow.yaml, so they survive reboots and are reloaded at boot.
// Reading them back tells us whether the machine already knows its geometry —
// i.e. whether the operator can skip the full calibration grid.

/// Frame anchor coordinates (mm), as stored in the firmware config.
#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq)]
pub struct Anchors {
    pub tl_x: f32,
    pub tl_y: f32,
    pub tr_x: f32,
    pub tr_y: f32,
    pub bl_x: f32,
    pub bl_y: f32,
    pub br_x: f32,
    pub br_y: f32,
    /// True when the geometry is non-degenerate and passes the firmware's own
    /// basic sanity checks (MaslowKinematics::checkBoundaries), i.e. the
    /// anchors are usable as a frame definition.
    pub valid: bool,
    /// True when the anchors are valid AND differ from the firmware's compiled
    /// defaults — i.e. a calibration actually ran and overwrote them. A factory
    /// machine reports the defaults, which pass `valid`, so `valid` alone is not
    /// proof of calibration; gate the "Calibrated ✓" badge and the
    /// calibration-skipping resume shortcut on this instead.
    pub calibrated: bool,
}

/// Parse a firmware config dump (the echo of `$/kinematics/MaslowKinematics/`)
/// into the anchor coordinates. Each line looks like
/// `$/kinematics/MaslowKinematics/tlX=-27.600`; we key off the last path
/// segment so the prefix is irrelevant. Returns None if no anchor key is found.
pub fn parse_anchors(dump: &str) -> Option<Anchors> {
    let mut a = Anchors::default();
    let mut found = false;

    for token in dump.split_whitespace() {
        let Some((key, val)) = token.split_once('=') else {
            continue;
        };
        // Key may be a full config path; only the trailing segment matters.
        let key = key.rsplit('/').next().unwrap_or(key);
        let Ok(v) = val.trim().parse::<f32>() else {
            continue;
        };
        let slot = match key {
            "tlX" => &mut a.tl_x,
            "tlY" => &mut a.tl_y,
            "trX" => &mut a.tr_x,
            "trY" => &mut a.tr_y,
            "blX" => &mut a.bl_x,
            "blY" => &mut a.bl_y,
            "brX" => &mut a.br_x,
            "brY" => &mut a.br_y,
            _ => continue,
        };
        *slot = v;
        found = true;
    }

    if !found {
        return None;
    }
    a.valid = anchors_valid(&a);
    a.calibrated = a.valid && !anchors_are_default(&a);
    Some(a)
}

// Firmware's compiled default anchor coordinates (MaslowKinematics.h). A machine
// that has never been calibrated reports exactly these; they pass `anchors_valid`,
// so we must detect them explicitly to avoid a false "Calibrated ✓".
const DEFAULT_TL_X: f32 = -27.6;
const DEFAULT_TL_Y: f32 = 2064.9;
const DEFAULT_TR_X: f32 = 2924.3;
const DEFAULT_TR_Y: f32 = 2066.5;
const DEFAULT_BR_X: f32 = 2953.2;

/// True when every reduced anchor coordinate matches the firmware default within
/// a tight tolerance — meaning the geometry was never overwritten by a
/// calibration. This is a safe-direction heuristic: a real calibration produces
/// measured floats that never coincide with all of these exact literals at once,
/// so the worst case is asking an already-calibrated user to recalibrate, never
/// the reverse (claiming calibrated when it is not).
fn anchors_are_default(a: &Anchors) -> bool {
    const EPS: f32 = 0.05;
    (a.tl_x - DEFAULT_TL_X).abs() < EPS
        && (a.tl_y - DEFAULT_TL_Y).abs() < EPS
        && (a.tr_x - DEFAULT_TR_X).abs() < EPS
        && (a.tr_y - DEFAULT_TR_Y).abs() < EPS
        && (a.br_x - DEFAULT_BR_X).abs() < EPS
}

/// Whether the anchor geometry is usable for cutting. Mirrors the firmware's
/// own boundary checks (MaslowKinematics.cpp): the top anchors must sit above
/// the bottom ones, left of right, and the frame must be non-degenerate. We do
/// NOT try to tell "freshly calibrated" from "default" here — the firmware does
/// not expose that distinction via config — only that valid anchors are loaded.
pub fn anchors_valid(a: &Anchors) -> bool {
    let non_zero = a.tl_y != 0.0 || a.tr_y != 0.0 || a.br_x != 0.0 || a.tr_x != 0.0;
    let top_above_bottom = a.tl_y > a.bl_y && a.tr_y > a.br_y;
    let left_of_right = a.tl_x < a.tr_x;
    let has_width = a.br_x > 0.0;
    non_zero && top_above_bottom && left_of_right && has_width
}

/// A calibration grid waypoint reported by the firmware.
#[derive(Serialize, Clone, Debug)]
pub struct Waypoint {
    pub n: usize,
    pub x: f32,
    pub y: f32,
}

/// Parse `[MSG:INFO: Waypoint N coordinates: X=.. Y=..]`.
pub fn parse_waypoint(line: &str) -> Option<Waypoint> {
    let rest = &line[line.find("Waypoint ")? + "Waypoint ".len()..];
    let n_end = rest.find(' ')?;
    let n: usize = rest[..n_end].parse().ok()?;

    let num = |key: &str| -> Option<f32> {
        let s = &rest[rest.find(key)? + key.len()..];
        let end = s
            .find(|c: char| c.is_whitespace() || c == ']')
            .unwrap_or(s.len());
        s[..end].parse::<f32>().ok()
    };

    Some(Waypoint {
        n,
        x: num("X=")?,
        y: num("Y=")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minfo() {
        let line = "MINFO: { \"homed\": true, \"calibrationInProgress\": false, \"tl\": 1234.5, \"tr\": 1230.1, \"br\": 1229.9, \"bl\": 1235.2, \"etl\": 0.1, \"etr\": -0.2, \"ebr\": 0.0, \"ebl\": 0.3, \"extended\": false }";
        let info = parse_minfo(line).unwrap();
        assert!(info.homed);
        assert!(!info.calibration_in_progress);
        assert_eq!(info.tl, Some(1234.5));
        assert_eq!(info.ebl, Some(0.3));
        assert!(!info.extended);
    }

    #[test]
    fn parses_minfo_with_nan_belt_reading() {
        // Firmware's `%g` formatting prints bare `nan`/`inf`/`-inf` for a
        // NaN/Inf belt reading, which is not valid JSON. The sanitizer must
        // turn just that field into `null` (-> None) and leave the rest of
        // the reading intact rather than failing the whole parse.
        let line = "MINFO: { \"homed\": true, \"calibrationInProgress\": false, \"tl\": nan, \"tr\": 1230.1, \"br\": inf, \"bl\": -inf, \"etl\": 0.1, \"etr\": -0.2, \"ebr\": 0.0, \"ebl\": 0.3, \"extended\": false }";
        let info = parse_minfo(line).expect("sanitized MINFO should parse");
        assert_eq!(info.tl, None);
        assert_eq!(info.br, None);
        assert_eq!(info.bl, None);
        assert_eq!(info.tr, Some(1230.1));
        assert_eq!(info.etl, Some(0.1));
        assert_eq!(info.etr, Some(-0.2));
        assert_eq!(info.ebr, Some(0.0));
        assert_eq!(info.ebl, Some(0.3));
        assert!(info.homed);
        assert!(!info.extended);
    }

    #[test]
    fn sanitizer_leaves_strings_and_normal_numbers_alone() {
        let json = "{ \"note\": \"contains inf and nan as text\", \"x\": -12.5, \"y\": 1.5e10 }";
        assert_eq!(sanitize_minfo_json(json), json);
    }

    #[test]
    fn rejects_non_minfo() {
        assert!(parse_minfo("ok").is_none());
        assert!(parse_minfo("<Idle|MPos:0,0,0>").is_none());
    }

    #[test]
    fn parses_current_state() {
        assert_eq!(parse_state("[MSG:INFO: Current state: 7]"), Some(7));
        assert_eq!(parse_state("[MSG:INFO: Current state: 0]"), Some(0));
        assert_eq!(parse_state("[MSG:INFO: something else]"), None);
    }

    #[test]
    fn policy_extended_allows_calibrate() {
        let p = policy_for(CalState::ExtendedOut);
        assert!(!p.busy);
        assert_eq!(p.label, "Extended");
        for a in ["retract", "extend", "takeSlack", "calibrate", "comply", "stop", "estop"] {
            assert!(p.allowed.contains(&a.to_string()), "missing {a}");
        }
    }

    #[test]
    fn policy_busy_allows_recovery_only() {
        // In a busy/transitional state the only user actions are the recovery
        // ones: Retract (the firmware's universal escape) + Stop/E-Stop.
        let p = policy_for(CalState::CalibrationInProgress);
        assert!(p.busy);
        assert_eq!(p.allowed, vec!["retract", "stop", "estop"]);
        for a in ["extend", "takeSlack", "calibrate", "comply"] {
            assert!(!p.allowed.contains(&a.to_string()), "{a} must be gated");
        }
    }

    #[test]
    fn policy_retract_allowed_while_extending() {
        // The stop-from-EXTENDING bug: $STOP freezes the FSM at EXTENDING(3).
        // Retract must remain offered so the user can recover.
        let p = policy_for(CalState::Extending);
        assert!(p.busy);
        assert!(p.allowed.contains(&"retract".to_string()));
        assert!(p.allowed.contains(&"stop".to_string()));
        assert!(p.allowed.contains(&"estop".to_string()));
        assert!(!p.allowed.contains(&"extend".to_string()));
    }

    #[test]
    fn policy_ready_to_cut_no_extend() {
        let p = policy_for(CalState::ReadyToCut);
        assert!(p.allowed.contains(&"calibrate".to_string()));
        assert!(!p.allowed.contains(&"extend".to_string()));
    }

    #[test]
    fn parses_waypoint() {
        let w = parse_waypoint("[MSG:INFO: Waypoint 3 coordinates: X=123.4 Y=-56.7]").unwrap();
        assert_eq!(w.n, 3);
        assert_eq!(w.x, 123.4);
        assert_eq!(w.y, -56.7);
    }

    #[test]
    fn transitions_match_firmware() {
        use CalState::*;
        // Valid per requestStateChange.
        assert!(valid_transition(Retracted, Extending));
        assert!(valid_transition(ExtendedOut, CalibrationInProgress));
        assert!(valid_transition(CalibrationInProgress, CalibrationComputing));
        assert!(valid_transition(CalibrationComputing, ReadyToCut));
        assert!(valid_transition(ReadyToCut, Retracting)); // retract from anywhere
        assert!(valid_transition(TakingSlack, TakingSlack)); // idempotent
        // Invalid.
        assert!(!valid_transition(Retracted, ExtendedOut)); // skips EXTENDING
        assert!(!valid_transition(Unknown, ReadyToCut));
    }

    #[test]
    fn tracker_first_and_valid() {
        use CalState::*;
        let mut t = StateTracker::default();
        assert_eq!(t.observe(Retracted, 0), Observation::First(Retracted));
        assert_eq!(t.observe(Retracted, 0), Observation::Unchanged);
        assert_eq!(
            t.observe(Extending, 0),
            Observation::Valid { from: Retracted, to: Extending }
        );
        assert_eq!(t.current(), Some(Extending));
    }

    #[test]
    fn tracker_straggler_then_discord() {
        use CalState::*;
        let mut t = StateTracker::default();
        t.observe(Retracted, 0); // current = Retracted
        // Invalid Retracted->ExtendedOut within a long debounce: suppressed.
        assert_eq!(
            t.observe(ExtendedOut, 10_000),
            Observation::Straggler { from: Retracted, to: ExtendedOut }
        );
        assert_eq!(t.current(), Some(Retracted));
        // Same invalid jump with no debounce: machine prevails, logged discord.
        assert_eq!(
            t.observe(ExtendedOut, 0),
            Observation::Discord { from: Retracted, to: ExtendedOut }
        );
        assert_eq!(t.current(), Some(ExtendedOut));
    }

    #[test]
    fn calibration_completion_from_in_progress_or_computing() {
        use CalState::*;
        // 6->7 and 9->7: the two firmware paths into READY_TO_CUT that end an
        // actual calibration run.
        assert!(is_calibration_completion(CalibrationInProgress, ReadyToCut));
        assert!(is_calibration_completion(CalibrationComputing, ReadyToCut));
    }

    #[test]
    fn calibration_completion_excludes_apply_tension() {
        use CalState::*;
        // 5->7: daily apply-tension cycle, not a calibration completion.
        assert!(!is_calibration_completion(TakingSlack, ReadyToCut));
    }

    #[test]
    fn calibration_completion_excludes_non_ready_targets() {
        use CalState::*;
        assert!(!is_calibration_completion(CalibrationInProgress, CalibrationComputing));
        assert!(!is_calibration_completion(ReadyToCut, ReadyToCut));
    }

    #[test]
    fn tracker_reports_completion_transitions_not_unchanged_or_apply_tension() {
        use CalState::*;
        // 6->7: calibration completion.
        let mut t = StateTracker::default();
        t.observe(CalibrationInProgress, 0);
        match t.observe(ReadyToCut, 0) {
            Observation::Valid { from, to } => assert!(is_calibration_completion(from, to)),
            other => panic!("expected Valid, got {other:?}"),
        }

        // 9->7: calibration completion via the computing path.
        let mut t = StateTracker::default();
        t.observe(CalibrationComputing, 0);
        match t.observe(ReadyToCut, 0) {
            Observation::Valid { from, to } => assert!(is_calibration_completion(from, to)),
            other => panic!("expected Valid, got {other:?}"),
        }

        // 5->7: apply-tension, must NOT read as completion.
        let mut t = StateTracker::default();
        t.observe(TakingSlack, 0);
        match t.observe(ReadyToCut, 0) {
            Observation::Valid { from, to } => assert!(!is_calibration_completion(from, to)),
            other => panic!("expected Valid, got {other:?}"),
        }

        // Unchanged re-reports of 7 must not be (mis)treated as a completion.
        let mut t = StateTracker::default();
        t.observe(ReadyToCut, 0); // First
        assert_eq!(t.observe(ReadyToCut, 0), Observation::Unchanged);
    }

    #[test]
    fn parses_anchor_dump() {
        // Format mirrors the firmware echo of `$/kinematics/MaslowKinematics/`.
        let dump = "$/kinematics/MaslowKinematics/tlX=-27.600\n\
                    $/kinematics/MaslowKinematics/tlY=2064.900\n\
                    $/kinematics/MaslowKinematics/trX=2924.300\n\
                    $/kinematics/MaslowKinematics/trY=2066.500\n\
                    $/kinematics/MaslowKinematics/blX=0.000\n\
                    $/kinematics/MaslowKinematics/blY=0.000\n\
                    $/kinematics/MaslowKinematics/brX=2953.200\n\
                    $/kinematics/MaslowKinematics/brY=0.000\nok\n";
        let a = parse_anchors(dump).unwrap();
        assert_eq!(a.tl_x, -27.6);
        assert_eq!(a.tr_y, 2066.5);
        assert_eq!(a.br_x, 2953.2);
        assert!(a.valid, "real frame geometry should be valid");
        // These ARE the firmware defaults, so geometry is valid but the machine
        // has not actually been calibrated.
        assert!(
            !a.calibrated,
            "default anchors must not read as calibrated"
        );
    }

    #[test]
    fn calibrated_anchors_differ_from_defaults() {
        // Values a real calibration would write — close to, but not exactly, the
        // defaults. Valid geometry AND not the default literals → calibrated.
        let dump = "tlX=-30.1 tlY=2061.2 trX=2921.8 trY=2069.7 \
                    blX=0 blY=0 brX=2950.5 brY=0";
        let a = parse_anchors(dump).unwrap();
        assert!(a.valid);
        assert!(a.calibrated, "measured anchors should read as calibrated");
    }

    #[test]
    fn default_anchors_are_detected() {
        let dump = "tlX=-27.600 tlY=2064.900 trX=2924.300 trY=2066.500 \
                    blX=0 blY=0 brX=2953.200 brY=0";
        let a = parse_anchors(dump).unwrap();
        assert!(a.valid);
        assert!(!a.calibrated, "exact firmware defaults are not a calibration");
    }

    #[test]
    fn anchors_zero_is_invalid() {
        // A never-calibrated / zeroed config is not usable.
        let a = Anchors::default();
        assert!(!anchors_valid(&a));
        let dump = "tlX=0 tlY=0 trX=0 trY=0 blX=0 blY=0 brX=0 brY=0";
        let parsed = parse_anchors(dump).unwrap();
        assert!(!parsed.valid);
    }

    #[test]
    fn anchors_rejects_non_dump() {
        assert!(parse_anchors("ok").is_none());
        assert!(parse_anchors("<Idle|MPos:0,0,0>").is_none());
    }

    #[test]
    fn anchors_geometry_guards() {
        // tlX must be left of trX, top above bottom.
        let mut a = Anchors {
            tl_x: 0.0,
            tl_y: 2000.0,
            tr_x: 3000.0,
            tr_y: 2000.0,
            bl_x: 0.0,
            bl_y: 0.0,
            br_x: 3000.0,
            br_y: 0.0,
            ..Default::default()
        };
        assert!(anchors_valid(&a));
        a.tl_x = 3500.0; // left now to the right of right anchor
        assert!(!anchors_valid(&a));
    }

    #[test]
    fn action_policy_idle_extended() {
        let p = action_policy("Idle", Some(CalState::ExtendedOut), false);
        assert!(p.jog && p.home);
        // Extended, but NOT tensioned: cutting is unsafe (XY belts unpowered).
        assert!(!p.run, "run must be gated until READY_TO_CUT");
        assert!(p.retract && p.extend && p.calibrate && p.comply);
        // Idle: nothing to feed-hold and nothing held, but the reset kill stays.
        assert!(!p.hold && !p.resume && p.reset);
    }

    #[test]
    fn action_policy_run_only_when_ready_to_cut() {
        // The firmware powers the XY belt PID only in READY_TO_CUT(7); a job may
        // start only there, and only while FluidNC is Idle.
        assert!(action_policy("Idle", Some(CalState::ReadyToCut), false).run);
        // Right calibration state but the machine is moving / alarmed.
        assert!(!action_policy("Run", Some(CalState::ReadyToCut), false).run);
        assert!(!action_policy("Alarm", Some(CalState::ReadyToCut), false).run);
        // Idle but not tensioned, or state unknown (e.g. just connected).
        for ms in [0, 2, 3, 4, 5, 6, 9] {
            let p = action_policy("Idle", Some(CalState::from_code(ms)), false);
            assert!(!p.run, "run in state {ms}");
        }
        assert!(!action_policy("Idle", None, false).run, "run with unknown state");
    }

    #[test]
    fn action_policy_homing_locks_motion() {
        // Belt op running: FluidNC reports Home, calibration busy.
        let p = action_policy("Home", Some(CalState::CalibrationInProgress), false);
        assert!(!p.jog && !p.home && !p.calibrate);
        // Retract stays live even mid-op (the firmware accepts it from any
        // state) — it is the recovery action.
        assert!(p.retract);
        assert!(p.stop && p.estop); // can still stop
        assert!(p.reset); // realtime always
    }

    #[test]
    fn action_policy_stop_from_extending_recovers() {
        // Reproduces the bug: $STOP set FluidNC back to Idle but left the Maslow
        // FSM frozen at EXTENDING(3). Retract + Stop/E-Stop must be offered so
        // the user is not stuck; extend/calibrate stay gated (busy state).
        let p = action_policy("Idle", Some(CalState::Extending), false);
        assert!(p.retract, "retract must recover a frozen EXTENDING state");
        assert!(p.stop && p.estop);
        assert!(!p.extend && !p.calibrate && !p.take_slack);
    }

    #[test]
    fn action_policy_busy_allows_stop_and_retract() {
        // Stop/E-Stop/Retract available across busy states regardless of FluidNC.
        for ms in [1, 3, 5, 6, 8, 9] {
            let p = action_policy("Run", Some(CalState::from_code(ms)), false);
            assert!(p.stop && p.estop, "stop/estop must be available in state {ms}");
            assert!(p.retract, "retract must be available in busy state {ms}");
        }
    }

    #[test]
    fn action_policy_job_locks_everything_but_realtime() {
        // Mid-cut (state Run): feed-hold and reset available, resume is not (not
        // held). Manual line actions are all locked out.
        let p = action_policy("Run", Some(CalState::ReadyToCut), true);
        assert!(!p.jog && !p.run && !p.retract && !p.stop);
        assert!(p.hold && !p.resume && p.reset);
    }

    #[test]
    fn action_policy_resume_only_when_held() {
        // Held: resume is offered, a fresh feed-hold is not, reset always.
        let p = action_policy("Hold", Some(CalState::ReadyToCut), true);
        assert!(p.resume && !p.hold && p.reset);
    }
}
