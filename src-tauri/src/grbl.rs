// Parsing of GRBL / FluidNC realtime status reports.
// Format example:
//   <Idle|MPos:0.000,0.000,0.000|Bf:35,255|FS:0,0|WCO:0.000,0.000,0.000>
//   <Run|MPos:10.0,20.0,-5.0|FS:500,0|Ov:100,100,100>
// Mirrors the parsing logic of ESP3D-WEBUI/www/js/grbl.js.

use serde::Serialize;

#[derive(Serialize, Clone, Debug, Default)]
pub struct MachineStatus {
    /// Machine state, e.g. "Idle", "Run", "Hold", "Alarm", "Jog", "Home".
    pub state: String,
    /// Optional substate code (e.g. Hold:0, Door:1).
    pub substate: Option<i32>,
    /// Machine position (absolute). Axis count varies; Maslow uses X,Y,Z.
    pub mpos: Vec<f32>,
    /// Work position (machine position minus work offset).
    pub wpos: Vec<f32>,
    /// Work coordinate offset, when reported.
    pub wco: Vec<f32>,
    /// Feed rate, spindle speed (from FS:).
    pub feed: f32,
    pub spindle: f32,
    /// Planner buffer blocks, RX serial bytes free (from Bf:).
    pub buffer_blocks: Option<i32>,
    pub buffer_bytes: Option<i32>,
    /// Feed / rapid / spindle overrides in percent (from Ov:).
    pub ov: Vec<i32>,
}

fn parse_floats(s: &str) -> Vec<f32> {
    s.split(',').filter_map(|v| v.trim().parse::<f32>().ok()).collect()
}

fn parse_ints(s: &str) -> Vec<i32> {
    s.split(',').filter_map(|v| v.trim().parse::<i32>().ok()).collect()
}

/// Returns Some(status) if the line contains a `<...>` status report, else None.
/// FluidNC may append extra tokens after the report (e.g. `<Idle|...>  [GC:...]`),
/// so we extract the bracketed section rather than matching the whole line.
pub fn parse_status_report(line: &str) -> Option<MachineStatus> {
    let start = line.find('<')?;
    let end = line[start..].find('>')? + start;
    let inner = &line[start + 1..end];
    let mut parts = inner.split('|');

    let mut status = MachineStatus::default();

    // First field is the state, possibly "State:substate".
    let state_field = parts.next()?;
    if let Some((name, code)) = state_field.split_once(':') {
        status.state = name.to_string();
        status.substate = code.trim().parse::<i32>().ok();
    } else {
        status.state = state_field.to_string();
    }

    for field in parts {
        if let Some((key, val)) = field.split_once(':') {
            match key {
                "MPos" => status.mpos = parse_floats(val),
                "WPos" => status.wpos = parse_floats(val),
                "WCO" => status.wco = parse_floats(val),
                "Bf" => {
                    let b = parse_ints(val);
                    status.buffer_blocks = b.first().copied();
                    status.buffer_bytes = b.get(1).copied();
                }
                "FS" => {
                    let fs = parse_floats(val);
                    status.feed = fs.first().copied().unwrap_or(0.0);
                    status.spindle = fs.get(1).copied().unwrap_or(0.0);
                }
                "F" => status.feed = parse_floats(val).first().copied().unwrap_or(0.0),
                "Ov" => status.ov = parse_ints(val),
                _ => {}
            }
        }
    }

    // Derive whichever of WPos / MPos is missing using WCO.
    if !status.wco.is_empty() {
        if status.wpos.is_empty() && !status.mpos.is_empty() {
            status.wpos = status
                .mpos
                .iter()
                .zip(&status.wco)
                .map(|(m, o)| m - o)
                .collect();
        } else if status.mpos.is_empty() && !status.wpos.is_empty() {
            status.mpos = status
                .wpos
                .iter()
                .zip(&status.wco)
                .map(|(w, o)| w + o)
                .collect();
        }
    }

    Some(status)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_idle_report() {
        let s = parse_status_report("<Idle|MPos:1.0,2.0,3.0|Bf:35,255|FS:0,0>").unwrap();
        assert_eq!(s.state, "Idle");
        assert_eq!(s.mpos, vec![1.0, 2.0, 3.0]);
        assert_eq!(s.buffer_blocks, Some(35));
        assert_eq!(s.buffer_bytes, Some(255));
    }

    #[test]
    fn parses_substate_and_wco() {
        let s = parse_status_report("<Hold:0|MPos:10.0,0.0,0.0|WCO:1.0,0.0,0.0|FS:500,1000>").unwrap();
        assert_eq!(s.state, "Hold");
        assert_eq!(s.substate, Some(0));
        assert_eq!(s.wpos, vec![9.0, 0.0, 0.0]);
        assert_eq!(s.feed, 500.0);
        assert_eq!(s.spindle, 1000.0);
    }

    #[test]
    fn rejects_non_status_lines() {
        assert!(parse_status_report("ok").is_none());
        assert!(parse_status_report("[MSG:INFO: hi]").is_none());
    }

    #[test]
    fn parses_real_maslow_5axis_with_trailing_gc() {
        // Captured from a real Maslow M4 (FluidNC v1.21).
        let line = "<Idle|MPos:0.000,0.000,73.000,0.000,0.000|FS:0,0|WCO:0.000,0.000,73.000,0.000,0.000>  [GC:G0 G54 G17 G21 G90 G94 M5 M9 T0 F0 S0]";
        let s = parse_status_report(line).unwrap();
        assert_eq!(s.state, "Idle");
        assert_eq!(s.mpos.len(), 5);
        assert_eq!(s.mpos[2], 73.0);
        // wpos derived from mpos - wco
        assert_eq!(s.wpos[2], 0.0);
    }
}
