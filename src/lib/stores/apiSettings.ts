import { writable } from "svelte/store";
import { invoke } from "@tauri-apps/api/core";

export interface ApiSettingsView {
  enabled: boolean;
  port_http: number;
  port_grpc: number;
  has_key: boolean;
  listening: boolean;
}

/** Mirrors the Rust-side `api_settings.json`. Null until the first load. */
export const apiSettings = writable<ApiSettingsView | null>(null);

export async function refreshApiSettings(): Promise<void> {
  apiSettings.set(await invoke<ApiSettingsView>("get_api_settings"));
}

export async function setApiEnabled(enabled: boolean): Promise<void> {
  await invoke("set_api_enabled", { enabled });
  await refreshApiSettings();
}

/** Regenerates the API key and returns the plaintext value. This is the only
 * time the plaintext key is ever available: it is not cached here or
 * anywhere else, so the caller must display it immediately. */
export async function regenerateApiKey(): Promise<string> {
  const key = await invoke<string>("regenerate_api_key");
  await refreshApiSettings();
  return key;
}
