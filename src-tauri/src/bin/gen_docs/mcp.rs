//! MCP tools reference generator.
//!
//! Reads the exact same `rmcp::ToolRouter`s the running MCP server combines
//! in `McpServer::new` (`src-tauri/src/mcp/mod.rs`), via
//! `maslow_desktop_lib::mcp_tools_by_domain()`: a small additive, docs-only
//! export in `lib.rs` that calls the same `mcp::tool_routers_by_domain()` /
//! `ToolRouter::list_all()` this crate's own
//! `every_domain_tool_is_registered_exactly_once` test already uses to
//! verify tool registration, and converts the result to plain owned data
//! (name/description/JSON-schema) so this generator doesn't need `mcp` or
//! `rmcp` types to be part of the library crate's public API. Either way,
//! this generator is reading a live registry, not a hand-copied list: a tool
//! renamed, added, or dropped shows up here on the next run with no separate
//! page to remember to update by hand.

use maslow_desktop_lib::{mcp_tools_by_domain, McpToolInfo};
use serde_json::Value;
use std::path::Path;

use crate::grpc::GrpcModel;
use crate::http::HttpModel;
use crate::util;

pub fn load() -> Vec<(&'static str, Vec<McpToolInfo>)> {
    mcp_tools_by_domain()
}

pub fn write(model: &[(&'static str, Vec<McpToolInfo>)], out_dir: &Path, http: &HttpModel, grpc: &GrpcModel) {
    for (domain, tools) in model {
        let md = render_domain(domain, tools, http, grpc);
        util::write_file(&out_dir.join(format!("{domain}.md")), &md);
    }
}

fn render_domain(domain: &str, tools: &[McpToolInfo], http: &HttpModel, grpc: &GrpcModel) -> String {
    let mut out = String::new();
    out += &util::frontmatter(domain, &format!("{} tools (MCP)", title_case(domain)));
    out += &util::generated_banner(
        "the live `rmcp::ToolRouter` this crate's MCP server actually registers (`McpServer::tool_router_*` \
        in `src-tauri/src/mcp/`), via `ToolRouter::list_all()`",
    );
    out += "Each tool below is a thin wrapper over the identical operation the [HTTP](../http/machine.md) and \
        [gRPC](../grpc/machine.md) APIs expose: same underlying `MaslowService` method, same effect on the \
        machine, just a different transport. The **See also** line under each tool cross-references the other \
        two surfaces, found by matching the tool's name against the real HTTP route table and the compiled \
        proto descriptor (not a hand-maintained mapping, so it can't point at the wrong route).\n\n";

    if tools.is_empty() {
        out += "_No tools registered in this domain._\n";
        return out;
    }

    let mut sorted: Vec<&McpToolInfo> = tools.iter().collect();
    sorted.sort_by(|a, b| a.name.cmp(&b.name));

    for tool in sorted {
        out += &format!("## {}\n\n", tool.name);
        if !tool.description.is_empty() {
            out += &format!("{}\n\n", tool.description);
        }

        let mut refs = Vec::new();
        if let Some(route) = http.route_for_handler(&tool.name) {
            refs.push(format!("HTTP `{} {}`", route.method, route.path));
        }
        if let Some((service, method)) = grpc.method_for_tool_name(&tool.name) {
            refs.push(format!("gRPC `{}.{}`", service, method.name()));
        }
        if refs.is_empty() {
            out += "_No matching HTTP/gRPC operation found (this tool may be MCP-only)._\n\n";
        } else {
            out += &format!("**See also:** {}\n\n", refs.join(" &middot; "));
        }

        let props = tool.input_schema.get("properties").and_then(Value::as_object);
        let required: Vec<&str> = tool
            .input_schema
            .get("required")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();

        match props {
            Some(props) if !props.is_empty() => {
                out += "| Parameter | Type | Required | Description |\n|---|---|---|---|\n";
                let mut names: Vec<&String> = props.keys().collect();
                names.sort();
                for name in names {
                    let prop = &props[name];
                    let ty = schema_type(prop);
                    let req = if required.contains(&name.as_str()) { "yes" } else { "no" };
                    let desc = prop.get("description").and_then(Value::as_str).unwrap_or("");
                    out += &format!("| `{name}` | {ty} | {req} | {} |\n", util::md_escape(desc));
                }
                out += "\n";
            }
            _ => out += "_No parameters._\n\n",
        }
    }
    out
}

fn schema_type(v: &Value) -> String {
    if let Some(t) = v.get("type") {
        match t {
            Value::String(s) if s == "array" => {
                let items = v.get("items").map(schema_type).unwrap_or_else(|| "any".to_string());
                format!("array of {items}")
            }
            Value::String(s) => s.clone(),
            Value::Array(arr) => {
                let parts: Vec<&str> = arr.iter().filter_map(Value::as_str).collect();
                if parts.is_empty() {
                    "any".to_string()
                } else {
                    parts.join(" or ")
                }
            }
            _ => "any".to_string(),
        }
    } else if v.get("enum").is_some() {
        "enum".to_string()
    } else {
        "any".to_string()
    }
}

fn title_case(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}
