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

/// Human label for a calibration state code (mirrors MaslowEnums.h).
pub fn state_name(state: i32) -> &'static str {
    match state {
        0 => "Unknown",
        1 => "Retracting",
        2 => "Retracted",
        3 => "Extending",
        4 => "Extended",
        5 => "Taking Slack",
        6 => "Calibrating",
        7 => "Ready to Cut",
        8 => "Releasing Tension",
        9 => "Computing",
        _ => "Unknown",
    }
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
    fn state_names() {
        assert_eq!(state_name(7), "Ready to Cut");
        assert_eq!(state_name(4), "Extended");
        assert_eq!(state_name(99), "Unknown");
    }
}
