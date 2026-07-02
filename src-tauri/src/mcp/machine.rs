// MCP MachineService tools. Mirrors `http/machine.rs` one for one: every tool
// delegates to the same `MaslowService` method (or, for the ping / firmware-
// version tools, the same `http_api` free function) so the command strings
// live in exactly one place regardless of transport.

use crate::http_api;
use crate::mcp::{err, ok, ok_json, McpServer};
use crate::proto::maslow::v1 as pb;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Deserialize, JsonSchema)]
pub struct ConnectParams {
    /// Hostname or IP address of the machine, e.g. "maslow.local" or "192.168.1.50".
    pub host: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct JogParams {
    /// Relative distance to move on X, in millimeters.
    #[serde(default)]
    pub dx: f64,
    /// Relative distance to move on Y, in millimeters.
    #[serde(default)]
    pub dy: f64,
    /// Relative distance to move on Z, in millimeters.
    #[serde(default)]
    pub dz: f64,
    /// Feed rate for the jog move, in millimeters per minute.
    pub feed: f64,
}

#[derive(Deserialize, JsonSchema)]
pub struct ZeroParams {
    /// Axis letters to zero, e.g. ["X", "Y"]. Empty means the default X+Y zero.
    #[serde(default)]
    pub axes: Vec<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct SendLineParams {
    /// Raw G-code or `$`-command line to send to the machine.
    pub line: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct SendRealtimeParams {
    /// Single realtime byte to send (0-255), e.g. 24 (0x18) for the soft-reset.
    pub byte: u32,
}

#[derive(Deserialize, JsonSchema)]
pub struct WriteSettingParams {
    /// Full FluidNC config path, e.g. "axes/x/steps_per_mm" or "Maslow_Scale_X".
    pub path: String,
    /// New value to write, as a string.
    pub value: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct HostParams {
    /// Hostname or IP address of the machine.
    pub host: String,
}

#[tool_router(router = tool_router_machine, vis = "pub(crate)")]
impl McpServer {
    #[tool(
        description = "Get the machine's last known GRBL/FluidNC status report (state, position, feed, overrides). Read-only: returns the most recently observed status rather than querying the machine directly."
    )]
    async fn get_machine_status(&self) -> CallToolResult {
        let status = { self.svc.snapshot.read().unwrap().status.clone() };
        match status {
            Some(s) => ok_json(&pb::MachineStatus::from(s)),
            None => err("no status observed yet - is the machine connected?"),
        }
    }

    #[tool(
        description = "Get which machine actions are currently allowed, reconciled from the FluidNC state, the Maslow calibration state, and whether a job is streaming. Read-only; check this before calling a motion or belt action."
    )]
    async fn get_action_policy(&self) -> CallToolResult {
        let policy = { self.svc.snapshot.read().unwrap().action_policy.clone() };
        match policy {
            Some(p) => ok_json(&pb::ActionPolicy::from(p)),
            None => err("no status observed yet - is the machine connected?"),
        }
    }

    #[tool(
        description = "Get an aggregate snapshot of everything currently known about the machine (status, action policy, Maslow telemetry, anchors, config entries) in one call. Read-only; prefer this over polling individual getters when full context is needed."
    )]
    async fn get_snapshot(&self) -> CallToolResult {
        let snap = self.svc.snapshot.read().unwrap();
        ok_json(&crate::grpc::convert::snapshot_to_proto(&snap))
    }

    #[tool(
        description = "Connect to the machine over its WebSocket control channel. Must be called before any other machine action; does not itself move anything."
    )]
    async fn connect(&self, Parameters(req): Parameters<ConnectParams>) -> CallToolResult {
        match self.svc.connect(req.host).await {
            Ok(()) => ok(),
            Err(e) => err(e),
        }
    }

    #[tool(description = "Disconnect from the machine's WebSocket control channel.")]
    async fn disconnect(&self) -> CallToolResult {
        match self.svc.disconnect().await {
            Ok(()) => ok(),
            Err(e) => err(e),
        }
    }

    #[tool(
        description = "Jog the machine by a relative distance in millimeters at the given feed rate. The machine will physically move."
    )]
    async fn jog(&self, Parameters(req): Parameters<JogParams>) -> CallToolResult {
        match self.svc.jog(req.dx, req.dy, req.dz, req.feed).await {
            Ok(()) => ok(),
            Err(e) => err(e),
        }
    }

    #[tool(
        description = "Home the machine ($H). The machine will physically move to find its reference position."
    )]
    async fn home(&self) -> CallToolResult {
        match self.svc.home().await {
            Ok(()) => ok(),
            Err(e) => err(e),
        }
    }

    #[tool(
        description = "Unlock the machine from an alarm state ($X). Does not itself cause motion, but re-enables motion commands."
    )]
    async fn unlock(&self) -> CallToolResult {
        match self.svc.unlock().await {
            Ok(()) => ok(),
            Err(e) => err(e),
        }
    }

    #[tool(
        description = "Feed-hold the machine (realtime pause). The machine will physically stop in place; call resume to continue."
    )]
    async fn hold(&self) -> CallToolResult {
        match self.svc.hold().await {
            Ok(()) => ok(),
            Err(e) => err(e),
        }
    }

    #[tool(
        description = "Resume the machine from a feed-hold. The machine will physically resume its previous motion."
    )]
    async fn resume(&self) -> CallToolResult {
        match self.svc.resume().await {
            Ok(()) => ok(),
            Err(e) => err(e),
        }
    }

    #[tool(
        description = "Zero the given axes at the machine's current position (sets the work coordinate offset). Does not cause motion, but changes how subsequent positions are reported."
    )]
    async fn zero(&self, Parameters(req): Parameters<ZeroParams>) -> CallToolResult {
        match self.svc.zero(req.axes).await {
            Ok(()) => ok(),
            Err(e) => err(e),
        }
    }

    #[tool(
        description = "Retract the Maslow belts. The machine will physically wind in all four belts; this is also the universal recovery action from a stuck calibration state."
    )]
    async fn retract(&self) -> CallToolResult {
        match self.svc.retract().await {
            Ok(()) => ok(),
            Err(e) => err(e),
        }
    }

    #[tool(
        description = "Extend the Maslow belts out to the frame corners. The machine will physically pay out all four belts."
    )]
    async fn extend(&self) -> CallToolResult {
        match self.svc.extend().await {
            Ok(()) => ok(),
            Err(e) => err(e),
        }
    }

    #[tool(
        description = "Take up slack in the Maslow belts to apply cutting tension. The machine will physically move the sled."
    )]
    async fn take_slack(&self) -> CallToolResult {
        match self.svc.take_slack().await {
            Ok(()) => ok(),
            Err(e) => err(e),
        }
    }

    #[tool(
        description = "Put the Maslow belts into a compliant, low-tension state for manual handling. The machine will physically release belt tension."
    )]
    async fn comply(&self) -> CallToolResult {
        match self.svc.comply().await {
            Ok(()) => ok(),
            Err(e) => err(e),
        }
    }

    #[tool(
        description = "Run the Maslow anchor calibration routine. The machine will physically move the sled through the calibration grid; this can take several minutes."
    )]
    async fn calibrate(&self) -> CallToolResult {
        match self.svc.calibrate().await {
            Ok(()) => ok(),
            Err(e) => err(e),
        }
    }

    #[tool(
        description = "Stop the current Maslow belt operation ($STOP). Halts belt motion but does not reset the FluidNC controller or the Maslow calibration state machine."
    )]
    async fn stop(&self) -> CallToolResult {
        match self.svc.stop().await {
            Ok(()) => ok(),
            Err(e) => err(e),
        }
    }

    #[tool(
        description = "Trigger the latching Maslow belt emergency stop ($ESTOP). The machine will immediately halt all belt motion; use for safety-critical stops."
    )]
    async fn e_stop(&self) -> CallToolResult {
        match self.svc.estop().await {
            Ok(()) => ok(),
            Err(e) => err(e),
        }
    }

    #[tool(
        description = "Send a raw G-code or $-command line to the machine. The machine will physically act on whatever the line instructs; use with care since this bypasses the higher-level action tools' intent."
    )]
    async fn send_line(&self, Parameters(req): Parameters<SendLineParams>) -> CallToolResult {
        match self.svc.send_line(req.line).await {
            Ok(()) => ok(),
            Err(e) => err(e),
        }
    }

    #[tool(
        description = "Send a single raw realtime control byte to the machine (e.g. 24 / 0x18 to soft-reset). Out-of-band and takes effect immediately, even mid-job."
    )]
    async fn send_realtime(&self, Parameters(req): Parameters<SendRealtimeParams>) -> CallToolResult {
        let byte: u8 = match req.byte.try_into() {
            Ok(b) => b,
            Err(_) => return err("byte must fit in a single byte (0-255)"),
        };
        match self.svc.send_realtime(byte).await {
            Ok(()) => ok(),
            Err(e) => err(e),
        }
    }

    #[tool(
        description = "Write a single FluidNC configuration setting to the machine's runtime config. This changes machine behavior (e.g. steps-per-mm, work area size) and does not persist across reboot unless save_config is also called."
    )]
    async fn write_setting(&self, Parameters(req): Parameters<WriteSettingParams>) -> CallToolResult {
        match self.svc.write_setting(req.path, req.value).await {
            Ok(()) => ok(),
            Err(e) => err(e),
        }
    }

    #[tool(description = "Persist the machine's current runtime configuration to flash so it survives a reboot.")]
    async fn save_config(&self) -> CallToolResult {
        match self.svc.save_config().await {
            Ok(()) => ok(),
            Err(e) => err(e),
        }
    }

    #[tool(
        description = "Test whether a machine is reachable at the given host and fetch basic firmware info. Read-only network probe; does not require an active connection."
    )]
    async fn ping_machine(&self, Parameters(req): Parameters<HostParams>) -> CallToolResult {
        let result = http_api::ping_machine(req.host).await;
        ok_json(&pb::PingMachineResponse {
            reachable: result.reachable,
            status: u32::from(result.status),
            info: result.info,
        })
    }

    #[tool(
        description = "Get the FluidNC firmware version reported by the machine at the given host. Read-only network probe; does not require an active connection."
    )]
    async fn get_firmware_version(&self, Parameters(req): Parameters<HostParams>) -> CallToolResult {
        let version = http_api::firmware_version(req.host).await;
        ok_json(&pb::GetFirmwareVersionResponse { version })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The exact JSON shape an MCP client sends for a jog call: `dz` omitted
    /// entirely relies on `#[serde(default)]`, matching the optional axes an
    /// LLM caller is likely to leave out of a 2D jog.
    #[test]
    fn jog_params_default_missing_axes_to_zero() {
        let params: JogParams = serde_json::from_value(serde_json::json!({"dx": 12.5, "feed": 600.0})).unwrap();
        assert_eq!(params.dx, 12.5);
        assert_eq!(params.dy, 0.0);
        assert_eq!(params.dz, 0.0);
        assert_eq!(params.feed, 600.0);
    }

    #[test]
    fn zero_params_defaults_to_empty_axes() {
        let params: ZeroParams = serde_json::from_value(serde_json::json!({})).unwrap();
        assert!(params.axes.is_empty());
    }

    #[test]
    fn send_realtime_params_rejects_out_of_range_byte() {
        let params: SendRealtimeParams = serde_json::from_value(serde_json::json!({"byte": 300})).unwrap();
        let converted: Result<u8, _> = params.byte.try_into();
        assert!(converted.is_err(), "300 must not fit in a u8");
    }

    #[test]
    fn send_realtime_params_accepts_soft_reset_byte() {
        let params: SendRealtimeParams = serde_json::from_value(serde_json::json!({"byte": 0x18})).unwrap();
        let converted: u8 = params.byte.try_into().unwrap();
        assert_eq!(converted, 0x18);
    }

    #[test]
    fn connect_params_requires_host() {
        let result: Result<ConnectParams, _> = serde_json::from_value(serde_json::json!({}));
        assert!(result.is_err(), "host is a required field, not defaulted");
    }
}
