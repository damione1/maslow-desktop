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

fn label_for(code: i32) -> &'static str {
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
/// `Calibration::requestStateChange()` in the firmware. The raw guards allow
/// some actions from transitional states too, but we restrict user actions to
/// stable states for predictability — Stop / E-Stop remain available always.
pub fn policy_for(code: i32) -> StatePolicy {
    let busy = matches!(
        code,
        RETRACTING | EXTENDING | TAKING_SLACK | CALIBRATION_IN_PROGRESS | RELEASE_TENSION | CALIBRATION_COMPUTING
    );

    // Stop / E-Stop are accepted in any state by the firmware.
    let mut allowed: Vec<String> = vec!["stop".into(), "estop".into()];

    if !busy {
        // RETRACTING is accepted from any state.
        allowed.push("retract".into());
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
    fn policy_busy_only_stop() {
        let p = policy_for(6); // CALIBRATION_IN_PROGRESS
        assert!(p.busy);
        assert_eq!(p.allowed, vec!["stop", "estop"]);
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
}
