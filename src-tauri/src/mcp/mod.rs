// MCP transport adapter: a third, parallel adapter over the same
// `MaslowService` (and free functions) the gRPC and HTTP layers call. Built on
// the official `rmcp` SDK's Streamable HTTP server transport, which exposes a
// `tower::Service` we nest directly into the HTTP gateway's own axum router
// (see `http::build_router`), so every MCP request rides the identical
// `auth_middleware` API-key gate the HTTP/gRPC transports already use rather
// than a second, separate auth path.
//
// One inherent `impl McpServer` block per domain (`machine`, `job`, `config`,
// `files`, `calibration`), each annotated with `#[tool_router]` to generate a
// per-domain `ToolRouter`; this file only combines them and builds the
// Streamable HTTP service to mount.

pub mod calibration;
pub mod config;
pub mod files;
pub mod job;
pub mod machine;

use crate::service::machine::MaslowService;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::model::{CallToolResult, ContentBlock};
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use rmcp::{tool_handler, ServerHandler};
use serde::Serialize;
use std::sync::Arc;

/// The MCP server handler. `StreamableHttpService` constructs one instance
/// per session (see `service()`'s factory closure below), each holding the
/// same shared `Arc<MaslowService>` the gRPC/HTTP adapters already delegate
/// to, so a tool call reaches the identical machine state.
pub struct McpServer {
    pub(crate) svc: Arc<MaslowService>,
    tool_router: ToolRouter<McpServer>,
}

impl McpServer {
    fn new(svc: Arc<MaslowService>) -> Self {
        Self {
            svc,
            // `#[tool_router(router = ...)]` in each domain file adds an
            // associated function of that name to this same `impl McpServer`
            // block (not a free function in that module), one per domain.
            tool_router: Self::tool_router_machine()
                + Self::tool_router_job()
                + Self::tool_router_config()
                + Self::tool_router_files()
                + Self::tool_router_calibration(),
        }
    }
}

#[tool_handler(
    router = self.tool_router,
    name = "maslow-desktop",
    instructions = "Controls a Maslow CNC router running FluidNC firmware. Tools whose \
        description says the machine will physically move are real, unsimulated actions on \
        hardware: check get_action_policy or get_snapshot first to confirm an action is \
        currently allowed before calling it."
)]
impl ServerHandler for McpServer {}

/// Build the Streamable HTTP transport (a `tower::Service`) to nest into the
/// HTTP gateway's shared, authenticated axum router at `/mcp`. Stateful mode
/// (the default) tracks one MCP session per client via the `Mcp-Session-Id`
/// header; each session gets its own `McpServer` instance from the factory
/// closure below, all sharing the same underlying `Arc<MaslowService>`.
pub fn service(svc: Arc<MaslowService>) -> StreamableHttpService<McpServer, LocalSessionManager> {
    StreamableHttpService::new(
        move || Ok(McpServer::new(svc.clone())),
        Arc::new(LocalSessionManager::default()),
        StreamableHttpServerConfig::default(),
    )
}

/// A plain-text success result carrying no payload, for action tools whose
/// only interesting outcome is "it worked" (matches the HTTP/gRPC layers'
/// empty response messages, e.g. `JogResponse {}`).
pub(crate) fn ok() -> CallToolResult {
    CallToolResult::success(vec![ContentBlock::text("ok")])
}

/// A successful result carrying `value` as structured JSON content, reusing
/// the same `pb::` (or domain) types and their existing `Serialize` impls the
/// gRPC/HTTP layers already return, so the JSON shape matches across all
/// three transports.
pub(crate) fn ok_json<T: Serialize>(value: &T) -> CallToolResult {
    match serde_json::to_value(value) {
        Ok(json) => CallToolResult::structured(json),
        Err(e) => err(format!("failed to serialize result: {e}")),
    }
}

/// A tool-level error: the request was valid and the tool ran, but the
/// underlying `MaslowService` call failed. Surfaced as `CallToolResult::error`
/// (caller-visible content) rather than a JSON-RPC protocol error, matching
/// the HTTP layer's `ApiError::internal` catch-all treatment of the same
/// `Result<_, String>` outcomes.
pub(crate) fn err(message: impl Into<String>) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(message.into())])
}

// The individual `#[tool_router(router = ...)]`-generated functions are
// associated functions on `McpServer`, not methods: building one needs no
// `McpServer` instance, only the type. That lets the registration checks
// below run without a live `Arc<MaslowService>`, which (like the rest of this
// crate's Tauri-backed state - see `http::tests` and `service::snapshot::tests`)
// needs a real `AppHandle` that isn't constructible in a unit test.
#[cfg(test)]
mod tests {
    use super::*;

    /// Every operation the HTTP/gRPC gateways expose (minus the streaming
    /// RPCs, deferred to a later PR) must show up as an MCP tool, under the
    /// exact snake_case name a caller would invoke. This is the completeness
    /// check for the whole PR: catches a domain file whose `#[tool_router]`
    /// never got wired into `McpServer::new`, and any tool silently dropped
    /// by a name collision across domains.
    #[test]
    fn every_domain_tool_is_registered_exactly_once() {
        let combined = McpServer::tool_router_machine()
            + McpServer::tool_router_job()
            + McpServer::tool_router_config()
            + McpServer::tool_router_files()
            + McpServer::tool_router_calibration();
        let names: Vec<String> = combined.list_all().into_iter().map(|t| t.name.to_string()).collect();

        let expected = [
            // machine
            "get_machine_status",
            "get_action_policy",
            "get_snapshot",
            "connect",
            "disconnect",
            "jog",
            "home",
            "unlock",
            "hold",
            "resume",
            "zero",
            "retract",
            "extend",
            "take_slack",
            "comply",
            "calibrate",
            "stop",
            "e_stop",
            "send_line",
            "send_realtime",
            "write_setting",
            "save_config",
            "ping_machine",
            "get_firmware_version",
            // job
            "get_job",
            "get_saved_job",
            "start_job",
            "pause_job",
            "resume_job",
            "stop_job",
            // config
            "list_config_entries",
            "get_config_entry",
            "update_config_entry",
            // files
            "list_files",
            "delete_file",
            "upload_file",
            "parse_sd_toolpath",
            "load_local_toolpath",
            // calibration
            "solve_calibration",
        ];

        for name in expected {
            assert!(names.contains(&name.to_string()), "missing tool: {name}");
        }
        assert_eq!(
            names.len(),
            expected.len(),
            "tool count changed: registered {names:?}, expected {} tools",
            expected.len()
        );
    }

    /// Every registered tool must carry a non-empty description: an LLM
    /// picks a tool by reading these, so a blank one would be unusable.
    #[test]
    fn every_tool_has_a_description() {
        let combined = McpServer::tool_router_machine()
            + McpServer::tool_router_job()
            + McpServer::tool_router_config()
            + McpServer::tool_router_files()
            + McpServer::tool_router_calibration();
        for tool in combined.list_all() {
            let description = tool.description.unwrap_or_default();
            assert!(!description.is_empty(), "tool {} has no description", tool.name);
        }
    }

    #[test]
    fn ok_returns_success_with_no_structured_content() {
        let result = ok();
        assert_eq!(result.is_error, Some(false));
        assert!(result.structured_content.is_none());
        assert!(!result.content.is_empty());
    }

    #[test]
    fn ok_json_wraps_value_as_structured_content() {
        #[derive(Serialize)]
        struct Payload {
            state: &'static str,
        }
        let result = ok_json(&Payload { state: "Idle" });
        assert_eq!(result.is_error, Some(false));
        assert_eq!(result.structured_content, Some(serde_json::json!({"state": "Idle"})));
    }

    #[test]
    fn err_returns_error_with_message_as_content() {
        let result = err("not connected to a machine");
        assert_eq!(result.is_error, Some(true));
        assert!(result.structured_content.is_none());
        assert_eq!(result.content.len(), 1);
    }
}
