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
#[allow(clippy::all)]
mod proto;
mod service;
mod streaming;
mod toolpath;

use service::machine::MaslowService;
use std::sync::Arc;
use tauri::Manager;

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
