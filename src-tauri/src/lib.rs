mod calibration;
mod connection;
mod fluidnc;
mod grbl;
mod http_api;
mod maslow;
mod streaming;

use connection::ConnState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(ConnState::default())
        .invoke_handler(tauri::generate_handler![
            calibration::solve_calibration,
            http_api::ping_machine,
            http_api::read_maslow_anchors,
            http_api::read_maslow_config,
            http_api::write_maslow_setting,
            http_api::save_maslow_config,
            http_api::upload_file,
            http_api::list_files,
            http_api::delete_file,
            connection::connect_ws,
            connection::disconnect_ws,
            connection::send_line,
            connection::send_realtime,
            connection::request_config_dump,
            connection::stream_start,
            connection::stream_pause,
            connection::stream_resume,
            connection::stream_stop,
            connection::stream_saved,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
