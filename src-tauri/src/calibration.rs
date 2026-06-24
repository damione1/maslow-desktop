// Client-side calibration anchor solver.
//
// Mirrors the firmware's on-device anchor recompute
// (firmware/FluidNC/src/Maslow/Calibration.cpp::recomputeAnchorsWithLevenbergMarquardt)
// so the desktop can re-solve a calibration from the raw belt measurements the
// firmware logs as `CLBM:[{bl,br,tr,tl},...]`. This lets the user verify a fit,
// reject a suspect waypoint and re-solve WITHOUT re-measuring, and write the
// resulting anchors back.
//
// Geometry (XY plane, Z already removed by the firmware before logging CLBM):
//   - BL anchor is fixed at the origin (0,0).
//   - BR anchor lies on the X axis: (brX, 0).
//   - Reduced parameter vector = [tlX, tlY, trX, trY, brX] (5 anchor params)
//     plus 2 sled-position params (sx, sy) per measurement.
// Forward model:
//   dTL = ‖sled - (tlX,tlY)‖, dTR = ‖sled - (trX,trY)‖,
//   dBL = ‖sled‖,            dBR = ‖sled - (brX,0)‖.
//
// The solver is exposed behind the `AnchorSolver` trait so future algorithms
// (robust variants, alternative parameterisations) can plug in without touching
// the event wiring or the front end.

use serde::{Deserialize, Serialize};

use crate::maslow::Anchors;

/// One belt-length measurement at a calibration waypoint (mm, XY plane).
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct Measurement {
    pub tl: f64,
    pub tr: f64,
    pub bl: f64,
    pub br: f64,
}

/// The reduced 5-parameter anchor estimate the LM optimises over. BL is fixed at
/// the origin and BR's Y is fixed at 0, so they are not parameters.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct AnchorParams {
    pub tl_x: f64,
    pub tl_y: f64,
    pub tr_x: f64,
    pub tr_y: f64,
    pub br_x: f64,
}

impl AnchorParams {
    /// Firmware default frame (MaslowKinematics defaults), used when no config
    /// anchors are available as a starting guess.
    pub fn firmware_default() -> Self {
        AnchorParams {
            tl_x: 0.0,
            tl_y: 2114.0,
            tr_x: 3500.0,
            tr_y: 2114.0,
            br_x: 3500.0,
        }
    }

    /// Expand the reduced params to the full 8-coordinate anchor set the rest of
    /// the app uses (BL at origin, BR on the X axis). `valid` is left false; the
    /// caller fills it via `maslow::anchors_valid`.
    pub fn to_anchors(self) -> Anchors {
        Anchors {
            tl_x: self.tl_x as f32,
            tl_y: self.tl_y as f32,
            tr_x: self.tr_x as f32,
            tr_y: self.tr_y as f32,
            bl_x: 0.0,
            bl_y: 0.0,
            br_x: self.br_x as f32,
            br_y: 0.0,
            valid: false,
        }
    }
}

impl From<&Anchors> for AnchorParams {
    fn from(a: &Anchors) -> Self {
        AnchorParams {
            tl_x: a.tl_x as f64,
            tl_y: a.tl_y as f64,
            tr_x: a.tr_x as f64,
            tr_y: a.tr_y as f64,
            br_x: a.br_x as f64,
        }
    }
}

/// Estimated sled position for a single measurement (mm, XY plane).
#[derive(Clone, Copy, Debug, Serialize, PartialEq)]
pub struct Sled {
    pub x: f64,
    pub y: f64,
}

/// Quality metrics for a solved fit (mirrors the firmware's CalibrationFitness).
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Fitness {
    /// sqrt(SSR / 4N) — overall belt-length error, mm.
    pub rms: f64,
    /// max|residual| — worst single belt-length error, mm.
    pub max_residual: f64,
    /// Per-anchor RMS [tl, tr, bl, br], mm.
    pub per_anchor: [f64; 4],
    /// True if LM converged within the iteration cap on at least one attempt.
    pub converged: bool,
}

/// Full result of a solve: candidate anchors + fitness + per-measurement detail.
/// `ok` reflects the firmware fitness gates; `gate_error` explains a failure.
#[derive(Clone, Debug, Serialize)]
pub struct SolveResult {
    pub solver: String,
    pub ok: bool,
    pub anchors: Anchors,
    pub params: AnchorParams,
    pub fitness: Fitness,
    pub sled: Vec<Sled>,
    /// Per-measurement residuals [tl, tr, bl, br] of the best params, mm.
    pub residuals: Vec<[f64; 4]>,
    pub gate_error: Option<String>,
}

/// Pluggable anchor-solving algorithm. Implementors take the raw belt
/// measurements plus an initial anchor guess and return candidate anchors with
/// fitness — even when the fitness gates fail (the caller inspects `ok`).
pub trait AnchorSolver {
    fn name(&self) -> &'static str;
    fn solve(&self, measurements: &[Measurement], initial: AnchorParams) -> SolveResult;
}

/// Resolve a solver by id. Currently only the firmware-equivalent LM solver;
/// add new algorithms here as additional match arms.
pub fn solver_by_name(name: &str) -> Option<Box<dyn AnchorSolver>> {
    match name {
        "levenberg-marquardt" | "" => Some(Box::new(LevenbergMarquardt)),
        _ => None,
    }
}

// ── Parsing ────────────────────────────────────────────────────────────────

/// Parse `CLBM:[{bl:.., br:.., tr:.., tl:..}, ...]` (optionally wrapped in a
/// `[MSG:INFO: ...]` log line) into the ordered measurement list.
pub fn parse_clbm(line: &str) -> Option<Vec<Measurement>> {
    let start = line.find("CLBM:[")? + "CLBM:".len();
    let bytes = &line[start..];
    let open = bytes.find('[')?;
    let close = bytes[open..].find(']')? + open;
    let body = &bytes[open + 1..close];

    let mut out = Vec::new();
    for chunk in body.split('}') {
        let chunk = chunk.trim_start_matches([',', ' ', '\t']);
        let Some(obj) = chunk.strip_prefix('{') else {
            continue;
        };
        let mut m = Measurement {
            tl: 0.0,
            tr: 0.0,
            bl: 0.0,
            br: 0.0,
        };
        let mut found = 0;
        for kv in obj.split(',') {
            let Some((k, v)) = kv.split_once(':') else {
                continue;
            };
            let Ok(val) = v.trim().parse::<f64>() else {
                continue;
            };
            match k.trim() {
                "tl" => m.tl = val,
                "tr" => m.tr = val,
                "bl" => m.bl = val,
                "br" => m.br = val,
                _ => continue,
            }
            found += 1;
        }
        if found == 4 {
            out.push(m);
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

/// Fitness numbers the firmware logs after a recompute, used as a test oracle.
#[derive(Clone, Debug, PartialEq)]
pub struct FirmwareFit {
    pub rms: f64,
    pub max_residual: f64,
    pub per_anchor: [f64; 4],
    pub converged: bool,
}

/// Parse `Find Anchors fit: rms=..mm max=..mm perAnchor=[..,..,..,..]mm converged=1`.
pub fn parse_fit_line(line: &str) -> Option<FirmwareFit> {
    if !line.contains("Find Anchors fit:") {
        return None;
    }
    let num_after = |key: &str| -> Option<f64> {
        let s = &line[line.find(key)? + key.len()..];
        let end = s
            .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+' || c == 'e'))
            .unwrap_or(s.len());
        s[..end].parse::<f64>().ok()
    };
    let per = {
        let s = &line[line.find("perAnchor=[")? + "perAnchor=[".len()..];
        let end = s.find(']')?;
        let mut it = s[..end].split(',').map(|t| t.trim().parse::<f64>().ok());
        [
            it.next()??,
            it.next()??,
            it.next()??,
            it.next()??,
        ]
    };
    Some(FirmwareFit {
        rms: num_after("rms=")?,
        max_residual: num_after("max=")?,
        per_anchor: per,
        converged: num_after("converged=").map(|v| v != 0.0).unwrap_or(false),
    })
}

/// Parse `Find Anchors recompute complete: tl=(x,y) tr=(x,y) brX=.. SSR=.. points=N`.
pub fn parse_recompute_line(line: &str) -> Option<AnchorParams> {
    if !line.contains("recompute complete") {
        return None;
    }
    let pair = |key: &str| -> Option<(f64, f64)> {
        let s = &line[line.find(key)? + key.len()..];
        let open = s.find('(')?;
        let close = s[open..].find(')')? + open;
        let (a, b) = s[open + 1..close].split_once(',')?;
        Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
    };
    let num_after = |key: &str| -> Option<f64> {
        let s = &line[line.find(key)? + key.len()..];
        let end = s
            .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+' || c == 'e'))
            .unwrap_or(s.len());
        s[..end].parse::<f64>().ok()
    };
    let (tl_x, tl_y) = pair("tl=")?;
    let (tr_x, tr_y) = pair("tr=")?;
    Some(AnchorParams {
        tl_x,
        tl_y,
        tr_x,
        tr_y,
        br_x: num_after("brX=")?,
    })
}

// ── Levenberg-Marquardt solver ───────────────────────────────────────────────

// Constants copied verbatim from Calibration.cpp.
const LM_INITIAL_LAMBDA: f64 = 0.001;
const LM_LAMBDA_INCREASE: f64 = 10.0;
const LM_LAMBDA_DECREASE: f64 = 0.1;
const LM_MAX_ITERATIONS: usize = 100;
const LM_MAX_REJECTIONS: i32 = 20;
const LM_CONVERGENCE_THRESHOLD: f64 = 1e-4;
const LM_MAX_RETRIES: usize = 10;
const LM_PERTURB_SMALL: f64 = 25.0;
const LM_PERTURB_LARGE: f64 = 50.0;
const LM_LAMBDA_OVERFLOW: f64 = 1e12;

const FITNESS_RMS_FAIL_MM: f64 = 5.0;
const FITNESS_MAX_RES_FAIL_MM: f64 = 15.0;

const PERTURB_X: [f64; LM_MAX_RETRIES] = [
    LM_PERTURB_SMALL,
    -LM_PERTURB_SMALL,
    0.0,
    0.0,
    LM_PERTURB_SMALL,
    -LM_PERTURB_SMALL,
    LM_PERTURB_LARGE,
    -LM_PERTURB_LARGE,
    0.0,
    0.0,
];
const PERTURB_Y: [f64; LM_MAX_RETRIES] = [
    0.0,
    0.0,
    LM_PERTURB_SMALL,
    -LM_PERTURB_SMALL,
    LM_PERTURB_SMALL,
    -LM_PERTURB_SMALL,
    0.0,
    0.0,
    LM_PERTURB_LARGE,
    -LM_PERTURB_LARGE,
];

/// Firmware-equivalent partitioned (Schur-complement) Levenberg-Marquardt.
pub struct LevenbergMarquardt;

impl AnchorSolver for LevenbergMarquardt {
    fn name(&self) -> &'static str {
        "levenberg-marquardt"
    }

    fn solve(&self, measurements: &[Measurement], initial: AnchorParams) -> SolveResult {
        lm_solve(measurements, initial)
    }
}

/// Gauss-Newton estimate of the sled position for one measurement (20 iters),
/// mirroring estimateSledPosition().
fn estimate_sled(m: &Measurement, p: &AnchorParams) -> (f64, f64) {
    let mut sx = (p.tl_x + p.tr_x) / 2.0;
    let mut sy = (p.tl_y + p.tr_y) / 4.0;

    let anchor_x = [p.tl_x, p.tr_x, 0.0, p.br_x];
    let anchor_y = [p.tl_y, p.tr_y, 0.0, 0.0];
    let belt = [m.tl, m.tr, m.bl, m.br];

    for _ in 0..20 {
        let (mut hxx, mut hyy, mut hxy, mut gx, mut gy) = (0.0, 0.0, 0.0, 0.0, 0.0);
        for j in 0..4 {
            let dx = sx - anchor_x[j];
            let dy = sy - anchor_y[j];
            let d = (dx * dx + dy * dy).sqrt() + 1e-10;
            let r = d - belt[j];
            let jx = dx / d;
            let jy = dy / d;
            hxx += jx * jx;
            hyy += jy * jy;
            hxy += jx * jy;
            gx += r * jx;
            gy += r * jy;
        }
        let det = hxx * hyy - hxy * hxy + 1e-12;
        sx -= (hyy * gx - hxy * gy) / det;
        sy -= (hxx * gy - hxy * gx) / det;
        if gx.abs() + gy.abs() < 1e-8 {
            break;
        }
    }
    (sx, sy)
}

/// Residuals [tl,tr,bl,br] for every measurement given the full parameter vector
/// (5 anchor params followed by 2 sled params per measurement).
fn bundle_residuals(measurements: &[Measurement], params: &[f64]) -> Vec<f64> {
    let (tl_x, tl_y) = (params[0], params[1]);
    let (tr_x, tr_y) = (params[2], params[3]);
    let br_x = params[4];

    let mut res = vec![0.0; measurements.len() * 4];
    for (i, m) in measurements.iter().enumerate() {
        let sx = params[5 + 2 * i];
        let sy = params[5 + 2 * i + 1];
        let d_tl = ((sx - tl_x).powi(2) + (sy - tl_y).powi(2)).sqrt();
        let d_tr = ((sx - tr_x).powi(2) + (sy - tr_y).powi(2)).sqrt();
        let d_bl = (sx * sx + sy * sy).sqrt();
        let d_br = ((sx - br_x).powi(2) + sy * sy).sqrt();
        res[4 * i] = d_tl - m.tl;
        res[4 * i + 1] = d_tr - m.tr;
        res[4 * i + 2] = d_bl - m.bl;
        res[4 * i + 3] = d_br - m.br;
    }
    res
}

fn sum_squared(res: &[f64]) -> f64 {
    res.iter().map(|r| r * r).sum()
}

/// Jacobians (anchors jia[4][5], sled jis[4][2]) and residuals ri[4] for one
/// measurement, mirroring measurementJacobiansAndResiduals().
fn jacobians(
    m: &Measurement,
    tl_x: f64,
    tl_y: f64,
    tr_x: f64,
    tr_y: f64,
    br_x: f64,
    sx: f64,
    sy: f64,
) -> ([[f64; 5]; 4], [[f64; 2]; 4], [f64; 4]) {
    let mut jia = [[0.0; 5]; 4];
    let mut jis = [[0.0; 2]; 4];

    let dx_tl = sx - tl_x;
    let dy_tl = sy - tl_y;
    let d_tl = (dx_tl * dx_tl + dy_tl * dy_tl).sqrt() + 1e-12;

    let dx_tr = sx - tr_x;
    let dy_tr = sy - tr_y;
    let d_tr = (dx_tr * dx_tr + dy_tr * dy_tr).sqrt() + 1e-12;

    let d_bl = (sx * sx + sy * sy).sqrt() + 1e-12;

    let dx_br = sx - br_x;
    let d_br = (dx_br * dx_br + sy * sy).sqrt() + 1e-12;

    let ri = [
        d_tl - m.tl,
        d_tr - m.tr,
        d_bl - m.bl,
        d_br - m.br,
    ];

    jia[0][0] = -dx_tl / d_tl;
    jia[0][1] = -dy_tl / d_tl;
    jis[0][0] = dx_tl / d_tl;
    jis[0][1] = dy_tl / d_tl;

    jia[1][2] = -dx_tr / d_tr;
    jia[1][3] = -dy_tr / d_tr;
    jis[1][0] = dx_tr / d_tr;
    jis[1][1] = dy_tr / d_tr;

    jis[2][0] = sx / d_bl;
    jis[2][1] = sy / d_bl;

    jia[3][4] = -dx_br / d_br;
    jis[3][0] = dx_br / d_br;
    jis[3][1] = sy / d_br;

    (jia, jis, ri)
}

/// Invert the damped 2x2 sled block [[d00, v01],[v01, d11]] with LM damping,
/// mirroring invertDamped2x2(). Returns None if singular.
fn invert_damped_2x2(v00: f64, v01: f64, v11: f64, lambda: f64) -> Option<[[f64; 2]; 2]> {
    let d00 = v00 + lambda * v00.max(1e-10);
    let d11 = v11 + lambda * v11.max(1e-10);
    let det = d00 * d11 - v01 * v01;
    if det.abs() < 1e-12 {
        return None;
    }
    let inv_det = 1.0 / det;
    Some([[d11 * inv_det, -v01 * inv_det], [-v01 * inv_det, d00 * inv_det]])
}

/// Gaussian elimination with partial pivoting on a 5x5 system, mirroring
/// solve5x5(). Returns None if singular.
fn solve_5x5(mut a: [[f64; 5]; 5], rhs: [f64; 5]) -> Option<[f64; 5]> {
    let mut b = rhs;
    for col in 0..5 {
        let mut pivot = col;
        for row in (col + 1)..5 {
            if a[row][col].abs() > a[pivot][col].abs() {
                pivot = row;
            }
        }
        if a[pivot][col].abs() < 1e-12 {
            return None;
        }
        if pivot != col {
            a.swap(col, pivot);
            b.swap(col, pivot);
        }
        let pivot_val = a[col][col];
        for row in (col + 1)..5 {
            let factor = a[row][col] / pivot_val;
            a[row][col] = 0.0;
            for k in (col + 1)..5 {
                a[row][k] -= factor * a[col][k];
            }
            b[row] -= factor * b[col];
        }
    }
    let mut sol = [0.0; 5];
    for col in (0..5).rev() {
        let mut s = b[col];
        for k in (col + 1)..5 {
            s -= a[col][k] * sol[k];
        }
        sol[col] = s / a[col][col];
    }
    Some(sol)
}

/// Outcome of a single LM attempt.
struct AttemptResult {
    best_params: Vec<f64>,
    best_ssr: f64,
    converged: bool,
}

/// Run one LM optimisation from a given starting parameter vector.
fn lm_attempt(measurements: &[Measurement], mut params: Vec<f64>) -> Option<AttemptResult> {
    let n = measurements.len();
    let mut residuals = bundle_residuals(measurements, &params);
    let mut current_ssr = sum_squared(&residuals);

    let mut best_params = params.clone();
    let mut best_ssr = current_ssr;
    let mut lambda = LM_INITIAL_LAMBDA;
    let mut rejections = 0;

    for _ in 0..LM_MAX_ITERATIONS {
        let (tl_x, tl_y) = (params[0], params[1]);
        let (tr_x, tr_y) = (params[2], params[3]);
        let br_x = params[4];

        let mut u = [[0.0f64; 5]; 5];
        let mut ga = [0.0f64; 5];
        let mut schur_sub = [[0.0f64; 5]; 5];
        let mut schur_gain = [0.0f64; 5];

        // First pass: accumulate reduced (Schur) normal equations over points.
        for i in 0..n {
            let sx = params[5 + 2 * i];
            let sy = params[5 + 2 * i + 1];
            let (jia, jis, ri) =
                jacobians(&measurements[i], tl_x, tl_y, tr_x, tr_y, br_x, sx, sy);

            let mut v00 = 0.0;
            let mut v01 = 0.0;
            let mut v11 = 0.0;
            let mut gi = [0.0f64; 2];
            let mut w = [[0.0f64; 2]; 5];
            for row in 0..4 {
                let js0 = jis[row][0];
                let js1 = jis[row][1];
                v00 += js0 * js0;
                v01 += js0 * js1;
                v11 += js1 * js1;
                gi[0] += js0 * ri[row];
                gi[1] += js1 * ri[row];
                for a in 0..5 {
                    let ja = jia[row][a];
                    ga[a] += ja * ri[row];
                    w[a][0] += ja * js0;
                    w[a][1] += ja * js1;
                    for b in a..5 {
                        u[a][b] += ja * jia[row][b];
                    }
                }
            }

            let inv_v = invert_damped_2x2(v00, v01, v11, lambda)?;
            for a in 0..5 {
                let winv0 = w[a][0] * inv_v[0][0] + w[a][1] * inv_v[1][0];
                let winv1 = w[a][0] * inv_v[0][1] + w[a][1] * inv_v[1][1];
                schur_gain[a] += winv0 * gi[0] + winv1 * gi[1];
                for b in a..5 {
                    schur_sub[a][b] += winv0 * w[b][0] + winv1 * w[b][1];
                }
            }
        }

        // Mirror the upper triangle into the lower for u and schur_sub.
        for a in 0..5 {
            for b in 0..a {
                u[a][b] = u[b][a];
                schur_sub[a][b] = schur_sub[b][a];
            }
        }

        let mut schur_matrix = [[0.0f64; 5]; 5];
        let mut schur_rhs = [0.0f64; 5];
        for a in 0..5 {
            for b in 0..5 {
                schur_matrix[a][b] = u[a][b] - schur_sub[a][b];
            }
            schur_matrix[a][a] += lambda * u[a][a].max(1e-10);
            schur_rhs[a] = -ga[a] + schur_gain[a];
        }

        let anchor_step = solve_5x5(schur_matrix, schur_rhs)?;

        let mut next_params = params.clone();
        for i in 0..5 {
            next_params[i] += anchor_step[i];
        }

        // Back-substitute the per-point sled steps.
        for i in 0..n {
            let sx = params[5 + 2 * i];
            let sy = params[5 + 2 * i + 1];
            let (jia, jis, ri) =
                jacobians(&measurements[i], tl_x, tl_y, tr_x, tr_y, br_x, sx, sy);

            let mut v00 = 0.0;
            let mut v01 = 0.0;
            let mut v11 = 0.0;
            let mut gi = [0.0f64; 2];
            let mut w = [[0.0f64; 2]; 5];
            for row in 0..4 {
                let js0 = jis[row][0];
                let js1 = jis[row][1];
                v00 += js0 * js0;
                v01 += js0 * js1;
                v11 += js1 * js1;
                gi[0] += js0 * ri[row];
                gi[1] += js1 * ri[row];
                for a in 0..5 {
                    w[a][0] += jia[row][a] * js0;
                    w[a][1] += jia[row][a] * js1;
                }
            }

            let inv_v = invert_damped_2x2(v00, v01, v11, lambda)?;
            let mut rhs0 = -gi[0];
            let mut rhs1 = -gi[1];
            for a in 0..5 {
                rhs0 -= w[a][0] * anchor_step[a];
                rhs1 -= w[a][1] * anchor_step[a];
            }
            let sx_step = inv_v[0][0] * rhs0 + inv_v[0][1] * rhs1;
            let sy_step = inv_v[1][0] * rhs0 + inv_v[1][1] * rhs1;
            next_params[5 + 2 * i] += sx_step;
            next_params[5 + 2 * i + 1] += sy_step;
        }

        let next_residuals = bundle_residuals(measurements, &next_params);
        let next_ssr = sum_squared(&next_residuals);

        if next_ssr < current_ssr {
            params = next_params;
            residuals = next_residuals;
            current_ssr = next_ssr;
            lambda = (lambda * LM_LAMBDA_DECREASE).max(1e-12);
            rejections = 0;

            if current_ssr < best_ssr {
                best_ssr = current_ssr;
                best_params = params.clone();
            }

            let step_norm: f64 = anchor_step.iter().map(|s| s * s).sum::<f64>().sqrt();
            if step_norm < LM_CONVERGENCE_THRESHOLD {
                break;
            }
        } else {
            lambda *= LM_LAMBDA_INCREASE;
            rejections += 1;
            if rejections > LM_MAX_REJECTIONS || lambda > LM_LAMBDA_OVERFLOW {
                break;
            }
        }
    }

    let _ = residuals;
    let converged = rejections <= LM_MAX_REJECTIONS && lambda < LM_LAMBDA_OVERFLOW;
    Some(AttemptResult {
        best_params,
        best_ssr,
        converged,
    })
}

fn lm_solve(measurements: &[Measurement], initial: AnchorParams) -> SolveResult {
    let n = measurements.len();
    let solver = "levenberg-marquardt".to_string();

    if n == 0 {
        return SolveResult {
            solver,
            ok: false,
            anchors: initial.to_anchors(),
            params: initial,
            fitness: Fitness {
                rms: f64::INFINITY,
                max_residual: f64::INFINITY,
                per_anchor: [f64::INFINITY; 4],
                converged: false,
            },
            sled: Vec::new(),
            residuals: Vec::new(),
            gate_error: Some("no measurements available".into()),
        };
    }

    let mut global_best: Option<Vec<f64>> = None;
    let mut global_best_ssr = f64::INFINITY;
    let mut any_converged = false;

    for attempt in 0..=LM_MAX_RETRIES {
        let (px, py) = if attempt > 0 {
            (PERTURB_X[attempt - 1], PERTURB_Y[attempt - 1])
        } else {
            (0.0, 0.0)
        };

        let perturbed = AnchorParams {
            tl_x: initial.tl_x + px,
            tl_y: initial.tl_y + py,
            tr_x: initial.tr_x - px, // symmetric: tr opposite to tl in X
            tr_y: initial.tr_y + py,
            br_x: initial.br_x,
        };

        let mut params = Vec::with_capacity(5 + 2 * n);
        params.push(perturbed.tl_x);
        params.push(perturbed.tl_y);
        params.push(perturbed.tr_x);
        params.push(perturbed.tr_y);
        params.push(perturbed.br_x);
        for m in measurements {
            let (sx, sy) = estimate_sled(m, &perturbed);
            params.push(sx);
            params.push(sy);
        }

        let Some(result) = lm_attempt(measurements, params) else {
            continue; // singular block — skip this attempt, mirrors firmware abort-per-attempt
        };

        if result.best_ssr < global_best_ssr {
            global_best_ssr = result.best_ssr;
            global_best = Some(result.best_params);
        }
        if result.converged {
            any_converged = true;
            break;
        }
    }

    let best = global_best.unwrap_or_else(|| {
        // No attempt produced a usable result; report the initial guess.
        let mut p = vec![initial.tl_x, initial.tl_y, initial.tr_x, initial.tr_y, initial.br_x];
        for m in measurements {
            let (sx, sy) = estimate_sled(m, &initial);
            p.push(sx);
            p.push(sy);
        }
        p
    });

    let params = AnchorParams {
        tl_x: best[0],
        tl_y: best[1],
        tr_x: best[2],
        tr_y: best[3],
        br_x: best[4],
    };

    let final_res = bundle_residuals(measurements, &best);
    let ssr = sum_squared(&final_res);
    let rms = (ssr / (4.0 * n as f64)).sqrt();
    let max_residual = final_res.iter().fold(0.0f64, |acc, r| acc.max(r.abs()));
    let mut per_anchor = [0.0f64; 4];
    for (j, slot) in per_anchor.iter_mut().enumerate() {
        let sum_sq: f64 = (0..n).map(|i| final_res[4 * i + j].powi(2)).sum();
        *slot = (sum_sq / n as f64).sqrt();
    }

    let fitness = Fitness {
        rms,
        max_residual,
        per_anchor,
        converged: any_converged,
    };

    // Firmware fitness gates.
    let gate_error = if !fitness.converged {
        Some("LM did not converge after retries (solver stalled); try calibrating again".into())
    } else if fitness.rms > FITNESS_RMS_FAIL_MM {
        Some(format!(
            "rms={:.3}mm exceeds limit {}mm",
            fitness.rms, FITNESS_RMS_FAIL_MM
        ))
    } else if fitness.max_residual > FITNESS_MAX_RES_FAIL_MM {
        Some(format!(
            "maxResidual={:.3}mm exceeds limit {}mm",
            fitness.max_residual, FITNESS_MAX_RES_FAIL_MM
        ))
    } else {
        None
    };

    let sled = (0..n)
        .map(|i| Sled {
            x: best[5 + 2 * i],
            y: best[5 + 2 * i + 1],
        })
        .collect();
    let residuals = (0..n)
        .map(|i| {
            [
                final_res[4 * i],
                final_res[4 * i + 1],
                final_res[4 * i + 2],
                final_res[4 * i + 3],
            ]
        })
        .collect();

    let mut anchors = params.to_anchors();
    anchors.valid = crate::maslow::anchors_valid(&anchors);

    SolveResult {
        solver,
        ok: gate_error.is_none(),
        anchors,
        params,
        fitness,
        sled,
        residuals,
        gate_error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Forward model: belt lengths for a sled at (sx,sy) given anchors. Mirrors
    /// bundle_residuals' distance terms (BL at origin, BR on X axis).
    fn synth(p: &AnchorParams, sx: f64, sy: f64) -> Measurement {
        Measurement {
            tl: ((sx - p.tl_x).powi(2) + (sy - p.tl_y).powi(2)).sqrt(),
            tr: ((sx - p.tr_x).powi(2) + (sy - p.tr_y).powi(2)).sqrt(),
            bl: (sx * sx + sy * sy).sqrt(),
            br: ((sx - p.br_x).powi(2) + sy * sy).sqrt(),
        }
    }

    fn truth() -> AnchorParams {
        AnchorParams {
            tl_x: -12.0,
            tl_y: 2389.0,
            tr_x: 3554.0,
            tr_y: 2370.0,
            br_x: 3500.0,
        }
    }

    /// A spread of sled positions across the work area.
    fn grid_points() -> Vec<(f64, f64)> {
        let mut v = Vec::new();
        for &x in &[600.0, 1200.0, 1750.0, 2300.0, 2900.0] {
            for &y in &[500.0, 1000.0, 1500.0] {
                v.push((x, y));
            }
        }
        v
    }

    #[test]
    fn parses_clbm() {
        let line = "[MSG:INFO: CLBM:[{bl:1234.5, br:2345.6, tr:3456.7, tl:4567.8},{bl:1.0, br:2.0, tr:3.0, tl:4.0}]]";
        let m = parse_clbm(line).unwrap();
        assert_eq!(m.len(), 2);
        assert_eq!(m[0], Measurement { tl: 4567.8, tr: 3456.7, bl: 1234.5, br: 2345.6 });
        assert_eq!(m[1], Measurement { tl: 4.0, tr: 3.0, bl: 1.0, br: 2.0 });
    }

    #[test]
    fn rejects_non_clbm() {
        assert!(parse_clbm("[MSG:INFO: Find Anchors complete]").is_none());
        assert!(parse_clbm("CLBM:[]").is_none());
    }

    #[test]
    fn parses_fit_and_recompute_oracle() {
        let fit = parse_fit_line(
            "[MSG:INFO: Find Anchors fit: rms=1.234mm max=4.56mm perAnchor=[1.1,2.2,3.3,4.4]mm converged=1]",
        )
        .unwrap();
        assert!((fit.rms - 1.234).abs() < 1e-9);
        assert!((fit.max_residual - 4.56).abs() < 1e-9);
        assert_eq!(fit.per_anchor, [1.1, 2.2, 3.3, 4.4]);
        assert!(fit.converged);

        let p = parse_recompute_line(
            "[MSG:INFO: Find Anchors recompute complete: tl=(-12.5,2389.1) tr=(3554.2,2370.3) brX=3500.4 SSR=12.3 points=9]",
        )
        .unwrap();
        assert!((p.tl_x - -12.5).abs() < 1e-9);
        assert!((p.tl_y - 2389.1).abs() < 1e-9);
        assert!((p.tr_x - 3554.2).abs() < 1e-9);
        assert!((p.tr_y - 2370.3).abs() < 1e-9);
        assert!((p.br_x - 3500.4).abs() < 1e-9);
    }

    #[test]
    fn recovers_known_anchors_from_clean_data() {
        let t = truth();
        let measurements: Vec<Measurement> =
            grid_points().iter().map(|&(x, y)| synth(&t, x, y)).collect();

        // Start from a perturbed but plausible guess.
        let initial = AnchorParams {
            tl_x: 0.0,
            tl_y: 2350.0,
            tr_x: 3500.0,
            tr_y: 2350.0,
            br_x: 3500.0,
        };
        let res = LevenbergMarquardt.solve(&measurements, initial);

        assert!(res.ok, "gates should pass on clean data: {:?}", res.gate_error);
        assert!(res.fitness.converged);
        assert!(res.fitness.rms < 1e-3, "rms={}", res.fitness.rms);
        assert!((res.params.tl_x - t.tl_x).abs() < 0.5, "tlX={}", res.params.tl_x);
        assert!((res.params.tl_y - t.tl_y).abs() < 0.5, "tlY={}", res.params.tl_y);
        assert!((res.params.tr_x - t.tr_x).abs() < 0.5, "trX={}", res.params.tr_x);
        assert!((res.params.tr_y - t.tr_y).abs() < 0.5, "trY={}", res.params.tr_y);
        assert!((res.params.br_x - t.br_x).abs() < 0.5, "brX={}", res.params.br_x);
        assert_eq!(res.sled.len(), measurements.len());
        assert_eq!(res.residuals.len(), measurements.len());
    }

    #[test]
    fn outlier_inflates_residual_then_excluding_it_recovers() {
        let t = truth();
        let pts = grid_points();
        let mut measurements: Vec<Measurement> =
            pts.iter().map(|&(x, y)| synth(&t, x, y)).collect();

        // Corrupt one waypoint by 30 mm on the TL belt.
        let bad = 4usize;
        measurements[bad].tl += 30.0;

        let initial = AnchorParams {
            tl_x: 0.0,
            tl_y: 2350.0,
            tr_x: 3500.0,
            tr_y: 2350.0,
            br_x: 3500.0,
        };

        let with_bad = LevenbergMarquardt.solve(&measurements, initial);
        // The corrupted point dominates the residual list.
        let worst = with_bad
            .residuals
            .iter()
            .enumerate()
            .max_by(|a, b| {
                let ra = a.1.iter().fold(0.0f64, |m, r| m.max(r.abs()));
                let rb = b.1.iter().fold(0.0f64, |m, r| m.max(r.abs()));
                ra.partial_cmp(&rb).unwrap()
            })
            .map(|(i, _)| i)
            .unwrap();
        assert_eq!(worst, bad, "outlier should be the worst residual");

        // Excluding it (what-if) restores a clean fit.
        let kept: Vec<Measurement> = measurements
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != bad)
            .map(|(_, m)| *m)
            .collect();
        let cleaned = LevenbergMarquardt.solve(&kept, initial);
        assert!(cleaned.ok, "excluding the outlier should pass gates: {:?}", cleaned.gate_error);
        assert!(cleaned.fitness.rms < 1e-3, "rms={}", cleaned.fitness.rms);
    }
}
