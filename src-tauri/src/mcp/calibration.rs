// MCP CalibrationService tool, mirroring `http/calibration.rs`. Pure compute,
// no connection state: calls the free `calibration::solve_calibration`
// function directly rather than a `MaslowService` method.

use crate::calibration;
use crate::maslow;
use crate::mcp::{err, ok_json, McpServer};
use crate::proto::maslow::v1 as pb;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Deserialize, JsonSchema)]
pub struct MeasurementParams {
    /// Top-left belt length, mm.
    pub tl: f64,
    /// Top-right belt length, mm.
    pub tr: f64,
    /// Bottom-left belt length, mm.
    pub bl: f64,
    /// Bottom-right belt length, mm.
    pub br: f64,
}

impl From<MeasurementParams> for calibration::Measurement {
    fn from(m: MeasurementParams) -> Self {
        calibration::Measurement { tl: m.tl, tr: m.tr, bl: m.bl, br: m.br }
    }
}

#[derive(Deserialize, JsonSchema)]
pub struct AnchorsParams {
    pub tl_x: f32,
    pub tl_y: f32,
    pub tr_x: f32,
    pub tr_y: f32,
    pub bl_x: f32,
    pub bl_y: f32,
    pub br_x: f32,
    pub br_y: f32,
}

impl From<AnchorsParams> for maslow::Anchors {
    fn from(a: AnchorsParams) -> Self {
        maslow::Anchors {
            tl_x: a.tl_x,
            tl_y: a.tl_y,
            tr_x: a.tr_x,
            tr_y: a.tr_y,
            bl_x: a.bl_x,
            bl_y: a.bl_y,
            br_x: a.br_x,
            br_y: a.br_y,
            valid: false,
            calibrated: false,
        }
    }
}

#[derive(Deserialize, JsonSchema)]
pub struct SolveCalibrationParams {
    /// Raw belt-length measurements at each calibration waypoint, in order.
    pub measurements: Vec<MeasurementParams>,
    /// Starting anchor guess, as frame anchors (matching what a config dump
    /// provides). If omitted, the solver falls back to the firmware default frame.
    #[serde(default)]
    pub initial: Option<AnchorsParams>,
    /// Zero-based measurement indices to exclude from the solve (what-if
    /// waypoint exclusion), without touching the machine.
    #[serde(default)]
    pub exclude: Vec<u32>,
    /// Solver identifier; currently only "levenberg-marquardt" is implemented.
    #[serde(default)]
    pub solver: Option<String>,
}

#[tool_router(router = tool_router_calibration, vis = "pub(crate)")]
impl McpServer {
    #[tool(
        description = "Solve Maslow frame anchor coordinates from raw belt-length measurements, without touching the machine. Pure computation, no physical side effects; useful to verify a fit or re-solve after excluding a suspect waypoint."
    )]
    async fn solve_calibration(&self, Parameters(req): Parameters<SolveCalibrationParams>) -> CallToolResult {
        let measurements: Vec<calibration::Measurement> = req.measurements.into_iter().map(Into::into).collect();
        let initial: Option<maslow::Anchors> = req.initial.map(Into::into);
        let exclude: Vec<usize> = req.exclude.into_iter().map(|i| i as usize).collect();

        match calibration::solve_calibration(measurements, initial, exclude, req.solver) {
            Ok(result) => ok_json(&pb::SolveResult::from(result)),
            Err(e) => err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Representative tool-call test: exercises the actual conversion path
    /// (`Parameters<SolveCalibrationParams>` deserialization -> domain types
    /// -> `calibration::solve_calibration`, the same free function the HTTP
    /// and gRPC layers call) end to end, without needing a live `McpServer`
    /// instance (which needs an `Arc<MaslowService>`, and in turn a real
    /// Tauri `AppHandle` unavailable in a unit test).
    #[test]
    fn solve_calibration_params_round_trip_into_a_solve() {
        let body = serde_json::json!({
            "measurements": [
                {"tl": 2380.0, "tr": 2360.0, "bl": 1200.0, "br": 1250.0},
                {"tl": 2200.0, "tr": 2500.0, "bl": 1400.0, "br": 1100.0},
                {"tl": 2600.0, "tr": 2100.0, "bl": 1000.0, "br": 1500.0},
            ],
            "exclude": [1u32],
        });
        let params: SolveCalibrationParams = serde_json::from_value(body).unwrap();
        assert_eq!(params.measurements.len(), 3);
        assert!(params.initial.is_none());
        assert_eq!(params.exclude, vec![1]);
        assert!(params.solver.is_none());

        let measurements: Vec<calibration::Measurement> = params.measurements.into_iter().map(Into::into).collect();
        let exclude: Vec<usize> = params.exclude.into_iter().map(|i| i as usize).collect();
        let result = calibration::solve_calibration(measurements, None, exclude, params.solver).unwrap();

        // Waypoint 1 was excluded, so only 2 of the 3 measurements were used.
        assert_eq!(result.kept_indices, vec![0, 2]);
    }

    #[test]
    fn anchors_params_convert_with_valid_and_calibrated_left_unset() {
        let a: maslow::Anchors = AnchorsParams {
            tl_x: -27.6,
            tl_y: 2064.9,
            tr_x: 2924.3,
            tr_y: 2066.5,
            bl_x: 0.0,
            bl_y: 0.0,
            br_x: 2953.2,
            br_y: 0.0,
        }
        .into();
        // `valid`/`calibrated` are derived by the solver, not caller-supplied.
        assert!(!a.valid);
        assert!(!a.calibrated);
        assert_eq!(a.tl_x, -27.6);
        assert_eq!(a.br_x, 2953.2);
    }
}
