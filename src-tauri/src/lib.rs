mod api_server;
mod api_settings;
mod auth;
mod calibration;
mod connection;
mod fluidnc;
mod grbl;
mod grpc;
mod http;
mod http_api;
mod maslow;
mod mcp;
#[allow(clippy::all)]
mod proto;
mod service;
mod streaming;
mod toolpath;

use service::machine::MaslowService;
use std::sync::Arc;
use tauri::Manager;

/// Plain-data snapshot of one registered MCP tool: name, description, and
/// its JSON-schema input shape, with no `rmcp` or `mcp::McpServer` types in
/// the signature. Exists solely so `bin/gen_docs` (a separate crate in this
/// same package, building the docs-site MCP reference) can read the tool
/// registry without this crate needing to make `mcp` (and everything it
/// touches, transitively including `MaslowService`) part of its public API.
#[derive(serde::Serialize)]
pub struct McpToolInfo {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// Every registered MCP tool, grouped by domain, as plain owned data read
/// from the same `rmcp::ToolRouter`s `mcp::McpServer::new` combines for the
/// running server (see `mcp::tool_routers_by_domain`). Only `bin/gen_docs`
/// calls this; the running app never does.
pub fn mcp_tools_by_domain() -> Vec<(&'static str, Vec<McpToolInfo>)> {
    mcp::tool_routers_by_domain()
        .into_iter()
        .map(|(domain, router)| {
            let tools = router
                .list_all()
                .into_iter()
                .map(|t| McpToolInfo {
                    name: t.name.to_string(),
                    description: t.description.map(|d| d.to_string()).unwrap_or_default(),
                    input_schema: serde_json::Value::Object((*t.input_schema).clone()),
                })
                .collect();
            (domain, tools)
        })
        .collect()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let svc = Arc::new(MaslowService::new(app.handle().clone()));
            app.manage(svc.clone());
            if svc.api_settings.read().unwrap().enabled {
                let svc = svc.clone();
                tauri::async_runtime::spawn(async move {
                    svc.start_api_servers().await;
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            calibration::solve_calibration,
            toolpath::load_toolpath,
            http_api::ping_machine,
            http_api::firmware_version,
            http_api::upload_file,
            http_api::list_files,
            http_api::delete_file,
            http_api::sd_toolpath,
            connection::connect_ws,
            connection::disconnect_ws,
            connection::send_line,
            connection::send_realtime,
            connection::ws_write_setting,
            connection::ws_save_config,
            connection::request_config_dump,
            connection::stream_start,
            connection::stream_pause,
            connection::stream_resume,
            connection::stream_stop,
            connection::stream_saved,
            api_settings::get_api_settings,
            api_settings::set_api_enabled,
            api_settings::regenerate_api_key,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
