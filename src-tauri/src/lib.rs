mod connection;
mod grbl;
mod http_api;

use connection::ConnState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(ConnState::default())
        .invoke_handler(tauri::generate_handler![
            http_api::ping_machine,
            connection::connect_ws,
            connection::disconnect_ws,
            connection::send_line,
            connection::send_realtime,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
