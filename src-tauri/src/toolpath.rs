// G-code → 2D toolpath for the job preview and the "trace boundary" action.
//
// Parses the XY motion of a G-code program into line segments (rapids vs feeds)
// plus a bounding box. Handles modal G0/G1/G2/G3, G90/G91 (absolute/incremental),
// G20/G21 (inch/mm) and arcs given by I/J centre offsets or an R radius. The Z
// axis and feed rates are ignored — this is a plan view, not a simulator.

use serde::Serialize;

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Segment {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
    /// True for G0 rapids, false for G1/G2/G3 cutting moves.
    pub rapid: bool,
    /// Index of the source line (into the same cleaned-line list the streamer
    /// uses), so live progress can highlight cut-so-far vs remaining.
    pub line: usize,
}

#[derive(Serialize, Clone, Debug, Default, PartialEq)]
pub struct Toolpath {
    pub segments: Vec<Segment>,
    pub min_x: f32,
    pub min_y: f32,
    pub max_x: f32,
    pub max_y: f32,
    /// False when the program produced no motion (empty bounding box).
    pub has_bounds: bool,
}

/// Tokenise one cleaned G-code line into (letter, value) words. Letters are
/// upper-cased; numbers are `[0-9.+-]` (no scientific notation — `E` is a word
/// letter, not an exponent). Inline `(...)` comments are skipped.
fn words(line: &str) -> Vec<(char, f64)> {
    let mut out = Vec::new();
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '(' {
            // Skip an inline parenthesised comment.
            while i < chars.len() && chars[i] != ')' {
                i += 1;
            }
            i += 1;
            continue;
        }
        if c.is_ascii_alphabetic() {
            let letter = c.to_ascii_uppercase();
            i += 1;
            let start = i;
            while i < chars.len()
                && (chars[i].is_ascii_digit() || matches!(chars[i], '.' | '+' | '-'))
            {
                i += 1;
            }
            let num: String = chars[start..i].iter().collect();
            if let Ok(v) = num.parse::<f64>() {
                out.push((letter, v));
            }
            continue;
        }
        i += 1;
    }
    out
}

/// Append the points of an arc from (x0,y0) to (x1,y1) about (cx,cy) to `pts`
/// (excluding the start point). `cw` selects G2 (clockwise) vs G3.
#[allow(clippy::too_many_arguments)]
fn arc_points(x0: f64, y0: f64, x1: f64, y1: f64, cx: f64, cy: f64, cw: bool, pts: &mut Vec<(f64, f64)>) {
    let r = ((x0 - cx).powi(2) + (y0 - cy).powi(2)).sqrt();
    let a0 = (y0 - cy).atan2(x0 - cx);
    let mut a1 = (y1 - cy).atan2(x1 - cx);
    let tau = std::f64::consts::TAU;
    if cw {
        // Clockwise = decreasing angle: force a1 <= a0.
        while a1 > a0 - 1e-9 {
            a1 -= tau;
        }
    } else {
        while a1 < a0 + 1e-9 {
            a1 += tau;
        }
    }
    let sweep = a1 - a0;
    let steps = ((sweep.abs() / (std::f64::consts::PI / 18.0)).ceil() as usize).max(2);
    for k in 1..=steps {
        let a = a0 + sweep * (k as f64 / steps as f64);
        pts.push((cx + r * a.cos(), cy + r * a.sin()));
    }
}

pub fn parse_toolpath(lines: &[String]) -> Toolpath {
    let mut tp = Toolpath::default();
    let (mut x, mut y) = (0.0_f64, 0.0_f64);
    let mut abs = true; // G90
    let mut inch = false; // G20
    let mut mode: i32 = 0; // modal motion: 0,1,2,3
    let mut seen_motion = false; // a motion mode has been established
    let (mut minx, mut miny) = (f64::INFINITY, f64::INFINITY);
    let (mut maxx, mut maxy) = (f64::NEG_INFINITY, f64::NEG_INFINITY);

    // The bounding box covers cutting moves only (feeds/arcs), not rapids, so it
    // is the part extent — what "trace boundary" should follow. The preview
    // auto-scales over all segments separately on the front end.
    let bump = |px: f64, py: f64, minx: &mut f64, miny: &mut f64, maxx: &mut f64, maxy: &mut f64| {
        *minx = minx.min(px);
        *miny = miny.min(py);
        *maxx = maxx.max(px);
        *maxy = maxy.max(py);
    };

    for (li, line) in lines.iter().enumerate() {
        let ws = words(line);
        if ws.is_empty() {
            continue;
        }

        let mut motion_this_line: Option<i32> = None;
        let (mut wx, mut wy, mut wi, mut wj, mut wr) =
            (None, None, None::<f64>, None::<f64>, None::<f64>);
        let scale = if inch { 25.4 } else { 1.0 };

        for (letter, v) in &ws {
            match letter {
                'G' => {
                    let g = v.round() as i32;
                    match g {
                        0..=3 => {
                            mode = g;
                            seen_motion = true;
                            motion_this_line = Some(g);
                        }
                        90 => abs = true,
                        91 => abs = false,
                        20 => inch = true,
                        21 => inch = false,
                        _ => {}
                    }
                }
                'X' => wx = Some(*v * scale),
                'Y' => wy = Some(*v * scale),
                'I' => wi = Some(*v * scale),
                'J' => wj = Some(*v * scale),
                'R' => wr = Some(*v * scale),
                _ => {}
            }
        }

        // Motion executes when a motion mode is active and there's an axis word
        // (modal), or an explicit G0-3 was given on the line.
        let has_axis = wx.is_some() || wy.is_some();
        if !seen_motion || (!has_axis && motion_this_line.is_none()) {
            continue;
        }

        let nx = match wx {
            Some(v) => if abs { v } else { x + v },
            None => x,
        };
        let ny = match wy {
            Some(v) => if abs { v } else { y + v },
            None => y,
        };

        if mode == 0 || mode == 1 {
            tp.segments.push(Segment {
                x0: x as f32,
                y0: y as f32,
                x1: nx as f32,
                y1: ny as f32,
                rapid: mode == 0,
                line: li,
            });
            if mode == 1 {
                bump(x, y, &mut minx, &mut miny, &mut maxx, &mut maxy);
                bump(nx, ny, &mut minx, &mut miny, &mut maxx, &mut maxy);
            }
        } else {
            // Arc. Centre from I/J (offsets from the start) or from an R radius.
            let centre = if wi.is_some() || wj.is_some() {
                Some((x + wi.unwrap_or(0.0), y + wj.unwrap_or(0.0)))
            } else if let Some(r) = wr {
                centre_from_radius(x, y, nx, ny, r, mode == 2)
            } else {
                None
            };
            match centre {
                Some((cx, cy)) => {
                    let mut pts = Vec::new();
                    arc_points(x, y, nx, ny, cx, cy, mode == 2, &mut pts);
                    let (mut px, mut py) = (x, y);
                    bump(x, y, &mut minx, &mut miny, &mut maxx, &mut maxy);
                    for (ax, ay) in pts {
                        tp.segments.push(Segment {
                            x0: px as f32,
                            y0: py as f32,
                            x1: ax as f32,
                            y1: ay as f32,
                            rapid: false,
                            line: li,
                        });
                        bump(ax, ay, &mut minx, &mut miny, &mut maxx, &mut maxy);
                        px = ax;
                        py = ay;
                    }
                }
                None => {
                    // No usable arc parameters — fall back to a straight feed.
                    tp.segments.push(Segment {
                        x0: x as f32,
                        y0: y as f32,
                        x1: nx as f32,
                        y1: ny as f32,
                        rapid: false,
                        line: li,
                    });
                    bump(x, y, &mut minx, &mut miny, &mut maxx, &mut maxy);
                    bump(nx, ny, &mut minx, &mut miny, &mut maxx, &mut maxy);
                }
            }
        }

        x = nx;
        y = ny;
    }

    if minx.is_finite() {
        tp.min_x = minx as f32;
        tp.min_y = miny as f32;
        tp.max_x = maxx as f32;
        tp.max_y = maxy as f32;
        tp.has_bounds = true;
    }
    tp
}

/// Compute an arc centre from endpoints and a signed radius (G2/G3 R-form).
/// Negative R selects the major arc. Returns None if the radius is too small.
fn centre_from_radius(x0: f64, y0: f64, x1: f64, y1: f64, r: f64, cw: bool) -> Option<(f64, f64)> {
    let dx = x1 - x0;
    let dy = y1 - y0;
    let d = (dx * dx + dy * dy).sqrt();
    if d < 1e-9 {
        return None;
    }
    let half = d / 2.0;
    let h2 = r * r - half * half;
    if h2 < -1e-6 {
        return None;
    }
    let h = h2.max(0.0).sqrt();
    let mx = (x0 + x1) / 2.0;
    let my = (y0 + y1) / 2.0;
    // Unit perpendicular to the chord.
    let ux = -dy / d;
    let uy = dx / d;
    // Sign selection mirrors the LinuxCNC/GRBL convention for R arcs.
    let sign = if (r < 0.0) ^ cw { 1.0 } else { -1.0 };
    Some((mx + sign * h * ux, my + sign * h * uy))
}

/// Read a local G-code file and parse it into a 2D toolpath for preview and the
/// trace-boundary action. Large files (100k+ lines) take real time to parse, so
/// the read + parse runs on a blocking thread and off the async runtime that
/// also drives the WebSocket connection loop.
#[tauri::command]
pub async fn load_toolpath(path: String) -> Result<Toolpath, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let lines = crate::streaming::load_gcode(&path)?;
        Ok(parse_toolpath(&lines))
    })
    .await
    .map_err(|e| format!("load_toolpath join: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn linear_moves_and_bounds() {
        let tp = parse_toolpath(&s(&[
            "G21 G90",
            "G0 X10 Y10",
            "G1 X40 Y10",
            "G1 X40 Y30",
            "X10 Y30", // modal G1
            "G1 X10 Y10",
        ]));
        assert_eq!(tp.segments.len(), 5);
        assert!(tp.segments[0].rapid);
        assert!(!tp.segments[1].rapid);
        assert!(tp.has_bounds);
        assert_eq!((tp.min_x, tp.min_y, tp.max_x, tp.max_y), (10.0, 10.0, 40.0, 30.0));
        // Source line indices track the cleaned-line list (0-based), so live
        // progress can compare against the streamer's acked count.
        assert_eq!(tp.segments[0].line, 1); // "G0 X10 Y10"
        assert_eq!(tp.segments[4].line, 5); // "G1 X10 Y10"
    }

    #[test]
    fn incremental_and_inch() {
        // G20 inch + G91 incremental: 1 inch right then 1 inch up from origin.
        let tp = parse_toolpath(&s(&["G20 G91", "G1 X1", "G1 Y1"]));
        assert_eq!(tp.segments.len(), 2);
        assert!((tp.segments[0].x1 - 25.4).abs() < 1e-3);
        assert!((tp.segments[1].y1 - 25.4).abs() < 1e-3);
        assert!((tp.max_x - 25.4).abs() < 1e-3 && (tp.max_y - 25.4).abs() < 1e-3);
    }

    #[test]
    fn arc_ij_quarter_circle() {
        // Quarter CCW arc from (10,0) to (0,10) about origin, r=10.
        let tp = parse_toolpath(&s(&["G90 G0 X10 Y0", "G3 X0 Y10 I-10 J0"]));
        // One rapid + several arc feed segments.
        assert!(tp.segments.iter().filter(|s| s.rapid).count() == 1);
        let arc: Vec<_> = tp.segments.iter().filter(|s| !s.rapid).collect();
        assert!(arc.len() >= 4, "arc should be subdivided");
        // All arc points lie ~10mm from the origin.
        for sg in &arc {
            let r = (sg.x1 * sg.x1 + sg.y1 * sg.y1).sqrt();
            assert!((r - 10.0).abs() < 0.2, "arc radius off: {r}");
        }
        assert!((tp.max_x - 10.0).abs() < 0.2 && (tp.max_y - 10.0).abs() < 0.2);
    }

    #[test]
    fn empty_program_has_no_bounds() {
        let tp = parse_toolpath(&s(&["G21 G90", "M5", "(comment only handled upstream)"]));
        assert!(!tp.has_bounds);
        assert!(tp.segments.is_empty());
    }
}
