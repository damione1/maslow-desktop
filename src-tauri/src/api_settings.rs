// Persisted API server settings: the enable/disable toggle and the hashed
// API key both the gRPC and HTTP transports authenticate requests against.
// Persistence mirrors the hand-rolled JSON read/write `streaming.rs` already
// uses for `current_job.json` (see `saved_path`/`persist`/`read_saved`
// there): plain `serde_json::to_string_pretty` + `std::fs::write`, and
// `std::fs::read_to_string` + `serde_json::from_str` on the way back in.
//
// Tauri commands for the (not-yet-built) frontend Settings tab live at the
// bottom of this file.

use crate::service::machine::MaslowService;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Manager};

#[derive(Clone, Serialize, Deserialize)]
pub struct ApiSettings {
    pub enabled: bool,
    /// Hex-encoded SHA-256 of the API key. Empty until a key is first
    /// generated. The plaintext key itself is never stored here or anywhere
    /// else on disk.
    pub api_key_hash: String,
    pub port_http: u16,
    pub port_grpc: u16,
}

impl Default for ApiSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            api_key_hash: String::new(),
            port_http: 8642,
            port_grpc: 50051,
        }
    }
}

fn settings_path(app: &AppHandle) -> Option<PathBuf> {
    let dir = app.path().app_data_dir().ok()?;
    Some(dir.join("api_settings.json"))
}

/// Persist settings to disk. Errors are swallowed, matching
/// `streaming::persist`: a failed write degrades to "the next read falls
/// back to defaults", not a failed command.
pub fn persist_settings(app: &AppHandle, settings: &ApiSettings) {
    let Some(path) = settings_path(app) else { return };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(settings) {
        let _ = std::fs::write(path, json);
    }
}

/// Parse settings JSON. Factored out of `read_settings` so the parsing logic
/// is unit testable without a real `AppHandle`, which (like the limitation
/// noted in `service/snapshot.rs`) cannot be constructed in this crate's
/// unit tests.
fn parse_settings(data: &str) -> Option<ApiSettings> {
    serde_json::from_str(data).ok()
}

/// Read settings from disk, defaulting to "API disabled, no key yet" on any
/// error (missing file, corrupt JSON): a fresh install or a corrupted
/// settings file must never fail startup.
pub fn read_settings(app: &AppHandle) -> ApiSettings {
    settings_path(app)
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|data| parse_settings(&data))
        .unwrap_or_default()
}

// --- Tauri commands -------------------------------------------------------
//
// Management surface for a future frontend Settings tab. Not wired into any
// UI yet.

#[derive(Serialize)]
pub struct ApiSettingsView {
    pub enabled: bool,
    pub port_http: u16,
    pub port_grpc: u16,
    /// True once a key has ever been generated. There is no way to show a
    /// masked form of the real key: only its hash is ever stored.
    pub has_key: bool,
    /// Whether the servers are actually accepting connections right now,
    /// not just whether `enabled` is set.
    pub listening: bool,
}

#[tauri::command]
pub async fn get_api_settings(state: tauri::State<'_, Arc<MaslowService>>) -> Result<ApiSettingsView, String> {
    let (enabled, port_http, port_grpc, has_key) = {
        let settings = state.api_settings.read().unwrap();
        (
            settings.enabled,
            settings.port_http,
            settings.port_grpc,
            !settings.api_key_hash.is_empty(),
        )
    };
    let listening = state.api_server.is_running().await;
    Ok(ApiSettingsView { enabled, port_http, port_grpc, has_key, listening })
}

#[tauri::command]
pub async fn set_api_enabled(
    state: tauri::State<'_, Arc<MaslowService>>,
    app: AppHandle,
    enabled: bool,
) -> Result<(), String> {
    let snapshot = {
        let mut settings = state.api_settings.write().unwrap();
        settings.enabled = enabled;
        settings.clone()
    };
    persist_settings(&app, &snapshot);
    if enabled {
        state.start_api_servers().await;
    } else {
        state.stop_api_servers().await;
    }
    Ok(())
}

#[tauri::command]
pub async fn regenerate_api_key(
    state: tauri::State<'_, Arc<MaslowService>>,
    app: AppHandle,
) -> Result<String, String> {
    let key = crate::auth::generate_key();
    let hash = crate::auth::hash_key(&key);
    let snapshot = {
        let mut settings = state.api_settings.write().unwrap();
        settings.api_key_hash = hash;
        settings.clone()
    };
    persist_settings(&app, &snapshot);
    // Only time the plaintext key is ever returned to a caller.
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings_are_disabled_with_no_key() {
        let d = ApiSettings::default();
        assert!(!d.enabled);
        assert!(d.api_key_hash.is_empty());
        assert_eq!(d.port_http, 8642);
        assert_eq!(d.port_grpc, 50051);
    }

    #[test]
    fn parse_settings_rejects_corrupt_json() {
        assert!(parse_settings("not json").is_none());
        assert!(parse_settings("").is_none());
        assert!(parse_settings("{\"enabled\": true}").is_none());
    }

    #[test]
    fn parse_settings_accepts_valid_json_round_trip() {
        let original = ApiSettings {
            enabled: true,
            api_key_hash: "abc123".to_string(),
            port_http: 8642,
            port_grpc: 50051,
        };
        let json = serde_json::to_string_pretty(&original).unwrap();
        let parsed = parse_settings(&json).unwrap();
        assert!(parsed.enabled);
        assert_eq!(parsed.api_key_hash, "abc123");
        assert_eq!(parsed.port_http, 8642);
        assert_eq!(parsed.port_grpc, 50051);
    }
}
