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
    /// Belt lengths (mm) for each arm.
    pub tl: f32,
    pub tr: f32,
    pub br: f32,
    pub bl: f32,
    /// Belt position errors (mm) for each arm.
    pub etl: f32,
    pub etr: f32,
    pub ebr: f32,
    pub ebl: f32,
    pub extended: bool,
}

/// Parse a `MINFO: { ... }` telemetry line. Returns None if it is not a MINFO
/// line or the JSON is malformed (e.g. nan/inf values).
pub fn parse_minfo(line: &str) -> Option<MaslowInfo> {
    let json = line.trim().strip_prefix("MINFO:")?.trim();
    serde_json::from_str(json).ok()
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

// Calibration state codes (MaslowEnums.h).
const UNKNOWN: i32 = 0;
const RETRACTING: i32 = 1;
const RETRACTED: i32 = 2;
const EXTENDING: i32 = 3;
const EXTENDEDOUT: i32 = 4;
const TAKING_SLACK: i32 = 5;
const CALIBRATION_IN_PROGRESS: i32 = 6;
const READY_TO_CUT: i32 = 7;
const RELEASE_TENSION: i32 = 8;
const CALIBRATION_COMPUTING: i32 = 9;

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
    /// stop, estop.
    pub allowed: Vec<String>,
}

pub fn label_for(code: i32) -> &'static str {
    match code {
        UNKNOWN => "Unknown",
        RETRACTING => "Retracting",
        RETRACTED => "Retracted",
        EXTENDING => "Extending",
        EXTENDEDOUT => "Extended",
        TAKING_SLACK => "Taking Slack",
        CALIBRATION_IN_PROGRESS => "Calibrating",
        READY_TO_CUT => "Ready to Cut",
        RELEASE_TENSION => "Releasing Tension",
        CALIBRATION_COMPUTING => "Computing",
        _ => "Unknown",
    }
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
pub fn policy_for(code: i32) -> StatePolicy {
    let busy = matches!(
        code,
        RETRACTING | EXTENDING | TAKING_SLACK | CALIBRATION_IN_PROGRESS | RELEASE_TENSION | CALIBRATION_COMPUTING
    );

    // Always available, even in busy/transitional states — see doc comment.
    // Retract is the universal escape hatch from a frozen FSM.
    let mut allowed: Vec<String> = vec!["retract".into(), "stop".into(), "estop".into()];

    if !busy {
        match code {
            RETRACTED => allowed.push("extend".into()),
            EXTENDEDOUT => {
                allowed.push("extend".into());
                allowed.push("takeSlack".into());
                allowed.push("calibrate".into());
                allowed.push("comply".into());
            }
            READY_TO_CUT => {
                allowed.push("takeSlack".into());
                allowed.push("calibrate".into());
                allowed.push("comply".into());
            }
            UNKNOWN => allowed.push("comply".into()),
            _ => {}
        }
    }

    StatePolicy {
        code,
        label: label_for(code).to_string(),
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
pub fn valid_transition(from: i32, to: i32) -> bool {
    if from == to {
        return true;
    }
    match to {
        // Accepted from any state by requestStateChange.
        UNKNOWN | RETRACTING => true,
        RETRACTED => from == RETRACTING,
        EXTENDING => matches!(from, RETRACTED | EXTENDEDOUT),
        EXTENDEDOUT => matches!(
            from,
            EXTENDING | TAKING_SLACK | RELEASE_TENSION | CALIBRATION_COMPUTING | CALIBRATION_IN_PROGRESS
        ),
        TAKING_SLACK => matches!(from, EXTENDEDOUT | READY_TO_CUT),
        CALIBRATION_IN_PROGRESS => matches!(from, EXTENDEDOUT | READY_TO_CUT | CALIBRATION_COMPUTING),
        CALIBRATION_COMPUTING => from == CALIBRATION_IN_PROGRESS,
        READY_TO_CUT => matches!(from, CALIBRATION_IN_PROGRESS | CALIBRATION_COMPUTING | TAKING_SLACK),
        RELEASE_TENSION => matches!(from, READY_TO_CUT | UNKNOWN | EXTENDEDOUT | CALIBRATION_COMPUTING),
        _ => false,
    }
}

/// Outcome of observing a reported state through the tracker.
#[derive(Debug, PartialEq)]
pub enum Observation {
    /// First state we have ever seen.
    First(i32),
    /// Same as the current state; nothing to do.
    Unchanged,
    /// A legitimate transition; state updated.
    Valid(i32),
    /// Invalid transition arriving within the debounce window — treated as a
    /// late straggler and ignored (state NOT updated).
    Straggler { from: i32, to: i32 },
    /// Invalid transition outside the debounce window — the machine's report
    /// prevails, so state IS updated, but the discordance is logged.
    Discord { from: i32, to: i32 },
}

/// Tracks the authoritative Maslow calibration state. The machine's report
/// always wins, but reports that contradict the transition graph and land just
/// after a change are suppressed as stragglers to avoid UI flicker.
pub struct StateTracker {
    current: Option<i32>,
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
    pub fn current(&self) -> Option<i32> {
        self.current
    }

    pub fn observe(&mut self, to: i32, debounce_ms: u64) -> Observation {
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
                    Observation::Valid(to)
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
pub fn action_policy(fluidnc: &str, maslow: Option<i32>, job_active: bool) -> ActionPolicy {
    let idle = fluidnc == "Idle";
    let alarm = fluidnc == "Alarm";
    let jogging = fluidnc == "Jog";
    // A stable state to *start* a belt/calibration op (these then drive Homing).
    let stable = idle || alarm;

    let mut p = ActionPolicy {
        // Realtime byte commands — injected out-of-band, safe even mid-job.
        hold: true,
        resume: true,
        reset: true,
        ..Default::default()
    };

    if !job_active {
        p.jog = idle || jogging; // $J= — notIdleOrJog
        p.home = idle || alarm; // $H — notIdleOrAlarm
        p.zero = idle; // G10 line
        p.run = idle; // start streaming
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
    /// machine has usable anchors and does NOT need recalibration to cut.
    pub valid: bool,
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
    Some(a)
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

// --- Full Maslow configuration --------------------------------------------
//
// The editable config screen exposes three families of firmware settings,
// confirmed against the firmware source AND validated against the real machine
// (FluidNC v1.21 on a Maslow M4):
//
//   * Anchor coordinates — `kinematics/MaslowKinematics/<key>` (12 items: the
//     TL/TR/BL/BR X/Y/Z corners), registered in MaslowKinematics::group().
//   * Work area — root items `Maslow_Work_Area_X/Y` and
//     `Maslow_Work_Area_Center_Offset_X/Y`, registered in
//     MachineConfig::groupM4Items() (the prefix `M` == "Maslow").
//   * Belt tension / extension — root items `Maslow_Retract_Current_Threshold`
//     and `Maslow_Extend_Dist`.
//
// NB: `Maslow_Apply_Tension_Belt_Retraction_Limit` and
// `Maslow_Apply_Tension_Allow_Limiting` were intentionally dropped: the real
// machine (v1.21) rejects them with `error:3` ("Invalid setting or command").
// They were only added to the firmware in commit 8088b960 (2026-06-02), which
// ships in v1.22.0 — so they do NOT exist as `$/` settings on v1.21. The reader
// also skips any key the firmware reports as invalid, so a newer/older firmware
// simply leaves the corresponding field at its default rather than failing the
// whole load.
//
// All are read with `$/<path>` (the firmware echoes `$/<path>=<value>`) and
// written with `$/<path>=<value>`; `$CO` then persists the config to flash.

/// The Maslow-relevant firmware settings we surface for display and editing.
/// Field names map 1:1 to the config keys (see the `path` table on the
/// frontend). Floats so we never lose precision on the wire; the firmware
/// stores the threshold as an int but accepts a float string just fine.
#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq)]
pub struct MaslowConfig {
    // Anchor coordinates (mm) — kinematics/MaslowKinematics/<key>.
    pub tl_x: f32,
    pub tl_y: f32,
    pub tl_z: f32,
    pub tr_x: f32,
    pub tr_y: f32,
    pub tr_z: f32,
    pub bl_x: f32,
    pub bl_y: f32,
    pub bl_z: f32,
    pub br_x: f32,
    pub br_y: f32,
    pub br_z: f32,
    // Work area (mm) — Maslow_Work_Area_*.
    pub work_area_x: f32,
    pub work_area_y: f32,
    pub work_area_center_offset_x: f32,
    pub work_area_center_offset_y: f32,
    // Belt tension / extension — Maslow_*.
    pub retract_current_threshold: f32,
    pub extend_dist: f32,
    /// Geometry sanity (same check as the `Anchors` badge).
    pub anchors_valid: bool,
}

/// Parse a concatenation of firmware setting echoes into a `MaslowConfig`.
/// Each recognised token looks like `$/<path>=<value>`; we key off the last
/// path segment for the kinematics group and off the full root key otherwise,
/// so the order and the exact section header lines are irrelevant. Unknown
/// keys and unparseable values are ignored. Returns None if no recognised key
/// was present at all (mirrors `parse_anchors`).
pub fn parse_maslow_config(dump: &str) -> Option<MaslowConfig> {
    let mut c = MaslowConfig::default();
    let mut found = false;

    for token in dump.split_whitespace() {
        let Some((key, val)) = token.split_once('=') else {
            continue;
        };
        // Drop any `$/` prefix and, for the nested kinematics keys, keep only
        // the trailing segment (e.g. `kinematics/MaslowKinematics/tlX` -> `tlX`).
        let key = key.trim_start_matches("$/");
        let short = key.rsplit('/').next().unwrap_or(key);
        let val = val.trim();

        let slot: &mut f32 = match (key, short) {
            (_, "tlX") => &mut c.tl_x,
            (_, "tlY") => &mut c.tl_y,
            (_, "tlZ") => &mut c.tl_z,
            (_, "trX") => &mut c.tr_x,
            (_, "trY") => &mut c.tr_y,
            (_, "trZ") => &mut c.tr_z,
            (_, "blX") => &mut c.bl_x,
            (_, "blY") => &mut c.bl_y,
            (_, "blZ") => &mut c.bl_z,
            (_, "brX") => &mut c.br_x,
            (_, "brY") => &mut c.br_y,
            (_, "brZ") => &mut c.br_z,
            ("Maslow_Work_Area_X", _) => &mut c.work_area_x,
            ("Maslow_Work_Area_Y", _) => &mut c.work_area_y,
            ("Maslow_Work_Area_Center_Offset_X", _) => &mut c.work_area_center_offset_x,
            ("Maslow_Work_Area_Center_Offset_Y", _) => &mut c.work_area_center_offset_y,
            ("Maslow_Retract_Current_Threshold", _) => &mut c.retract_current_threshold,
            ("Maslow_Extend_Dist", _) => &mut c.extend_dist,
            _ => continue,
        };
        let Ok(v) = val.parse::<f32>() else {
            continue;
        };
        *slot = v;
        found = true;
    }

    if !found {
        return None;
    }
    c.anchors_valid = anchors_valid(&Anchors {
        tl_x: c.tl_x,
        tl_y: c.tl_y,
        tr_x: c.tr_x,
        tr_y: c.tr_y,
        bl_x: c.bl_x,
        bl_y: c.bl_y,
        br_x: c.br_x,
        br_y: c.br_y,
        valid: false,
    });
    Some(c)
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
        assert_eq!(info.tl, 1234.5);
        assert_eq!(info.ebl, 0.3);
        assert!(!info.extended);
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
        let p = policy_for(4); // EXTENDEDOUT
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
        let p = policy_for(6); // CALIBRATION_IN_PROGRESS
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
        let p = policy_for(3); // EXTENDING
        assert!(p.busy);
        assert!(p.allowed.contains(&"retract".to_string()));
        assert!(p.allowed.contains(&"stop".to_string()));
        assert!(p.allowed.contains(&"estop".to_string()));
        assert!(!p.allowed.contains(&"extend".to_string()));
    }

    #[test]
    fn policy_ready_to_cut_no_extend() {
        let p = policy_for(7);
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
        // Valid per requestStateChange.
        assert!(valid_transition(2, 3)); // RETRACTED -> EXTENDING
        assert!(valid_transition(4, 6)); // EXTENDED -> CALIBRATION_IN_PROGRESS
        assert!(valid_transition(6, 9)); // IN_PROGRESS -> COMPUTING
        assert!(valid_transition(9, 7)); // COMPUTING -> READY_TO_CUT
        assert!(valid_transition(7, 1)); // retract from anywhere
        assert!(valid_transition(5, 5)); // idempotent
        // Invalid.
        assert!(!valid_transition(2, 4)); // RETRACTED -> EXTENDED (skips EXTENDING)
        assert!(!valid_transition(0, 7)); // UNKNOWN -> READY_TO_CUT
    }

    #[test]
    fn tracker_first_and_valid() {
        let mut t = StateTracker::default();
        assert_eq!(t.observe(2, 0), Observation::First(2));
        assert_eq!(t.observe(2, 0), Observation::Unchanged);
        assert_eq!(t.observe(3, 0), Observation::Valid(3));
        assert_eq!(t.current(), Some(3));
    }

    #[test]
    fn tracker_straggler_then_discord() {
        let mut t = StateTracker::default();
        t.observe(2, 0); // current = 2
        // Invalid 2->4 within a long debounce: suppressed straggler, state stays.
        assert_eq!(t.observe(4, 10_000), Observation::Straggler { from: 2, to: 4 });
        assert_eq!(t.current(), Some(2));
        // Same invalid jump with no debounce: machine prevails, logged discord.
        assert_eq!(t.observe(4, 0), Observation::Discord { from: 2, to: 4 });
        assert_eq!(t.current(), Some(4));
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
            valid: false,
        };
        assert!(anchors_valid(&a));
        a.tl_x = 3500.0; // left now to the right of right anchor
        assert!(!anchors_valid(&a));
    }

    #[test]
    fn parses_full_config() {
        // Kinematics section dump (anchors incl. Z) + the root Maslow_* keys,
        // exactly as the firmware echoes them for `$/kinematics/...` and
        // individual `$/Maslow_*` reads (the v1.21 set of valid keys).
        let dump = "/kinematics/MaslowKinematics:\n\
                    $/kinematics/MaslowKinematics/tlX=-27.600\n\
                    $/kinematics/MaslowKinematics/tlY=2064.900\n\
                    $/kinematics/MaslowKinematics/tlZ=100.000\n\
                    $/kinematics/MaslowKinematics/trX=2924.300\n\
                    $/kinematics/MaslowKinematics/trY=2066.500\n\
                    $/kinematics/MaslowKinematics/trZ=56.000\n\
                    $/kinematics/MaslowKinematics/blX=0.000\n\
                    $/kinematics/MaslowKinematics/blY=0.000\n\
                    $/kinematics/MaslowKinematics/blZ=34.000\n\
                    $/kinematics/MaslowKinematics/brX=2953.200\n\
                    $/kinematics/MaslowKinematics/brY=0.000\n\
                    $/kinematics/MaslowKinematics/brZ=78.000\n\
                    ok\n\
                    $/Maslow_Work_Area_X=2440.000\nok\n\
                    $/Maslow_Work_Area_Y=1220.000\nok\n\
                    $/Maslow_Work_Area_Center_Offset_X=0.000\nok\n\
                    $/Maslow_Work_Area_Center_Offset_Y=0.000\nok\n\
                    $/Maslow_Retract_Current_Threshold=1300\nok\n\
                    $/Maslow_Extend_Dist=1700.000\nok\n";
        let c = parse_maslow_config(dump).unwrap();
        assert_eq!(c.tl_x, -27.6);
        assert_eq!(c.tl_z, 100.0);
        assert_eq!(c.br_z, 78.0);
        assert_eq!(c.work_area_x, 2440.0);
        assert_eq!(c.work_area_y, 1220.0);
        assert_eq!(c.retract_current_threshold, 1300.0);
        assert_eq!(c.extend_dist, 1700.0);
        assert!(c.anchors_valid, "real frame geometry should be valid");
    }

    #[test]
    fn config_skips_invalid_keys_and_partial() {
        // A partial dump that also includes the firmware's rejection echoes for
        // the keys absent on v1.21 (`Maslow_Apply_Tension_*`). The error lines
        // carry no `=` token, so the parser ignores them: a single rejected key
        // must never break the whole load.
        let dump = "$/Maslow_Work_Area_X=1500\nok\n\
                    [MSG:ERR: Invalid setting or command: /Maslow_Apply_Tension_Belt_Retraction_Limit]\n\
                    error:3\n\
                    [MSG:ERR: Invalid setting or command: /Maslow_Apply_Tension_Allow_Limiting]\n\
                    error:3\n";
        let c = parse_maslow_config(dump).unwrap();
        assert_eq!(c.work_area_x, 1500.0);
        // Anchors absent -> default zeros -> geometry invalid.
        assert!(!c.anchors_valid);
    }

    #[test]
    fn config_rejects_non_dump() {
        assert!(parse_maslow_config("ok").is_none());
        assert!(parse_maslow_config("<Idle|MPos:0,0,0>").is_none());
        // A dump containing ONLY rejection echoes (every key invalid) yields
        // None rather than a bogus all-zero config.
        let only_errs = "[MSG:ERR: Invalid setting or command: /Maslow_Apply_Tension_Allow_Limiting]\nerror:3\n";
        assert!(parse_maslow_config(only_errs).is_none());
    }

    #[test]
    fn action_policy_idle_extended() {
        let p = action_policy("Idle", Some(4), false);
        assert!(p.jog && p.home && p.run);
        assert!(p.retract && p.extend && p.calibrate && p.comply);
        assert!(p.hold && p.resume && p.reset);
    }

    #[test]
    fn action_policy_homing_locks_motion() {
        // Belt op running: FluidNC reports Home, calibration busy.
        let p = action_policy("Home", Some(6), false);
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
        let p = action_policy("Idle", Some(3), false);
        assert!(p.retract, "retract must recover a frozen EXTENDING state");
        assert!(p.stop && p.estop);
        assert!(!p.extend && !p.calibrate && !p.take_slack);
    }

    #[test]
    fn action_policy_busy_allows_stop_and_retract() {
        // Stop/E-Stop/Retract available across busy states regardless of FluidNC.
        for ms in [1, 3, 5, 6, 8, 9] {
            let p = action_policy("Run", Some(ms), false);
            assert!(p.stop && p.estop, "stop/estop must be available in state {ms}");
            assert!(p.retract, "retract must be available in busy state {ms}");
        }
    }

    #[test]
    fn action_policy_job_locks_everything_but_realtime() {
        let p = action_policy("Run", Some(7), true);
        assert!(!p.jog && !p.run && !p.retract && !p.stop);
        assert!(p.hold && p.resume && p.reset);
    }
}
