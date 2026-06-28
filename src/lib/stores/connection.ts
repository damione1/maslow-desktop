import { writable } from "svelte/store";
import { invoke } from "@tauri-apps/api/core";

export type ConnState = "disconnected" | "testing" | "connected";

export interface ConnectionInfo {
  host: string;
  state: ConnState;
  /** Raw firmware info ([ESP420] output) from the last successful ping. */
  info: string;
  error: string;
}

const STORAGE_KEY = "maslow.host";

function loadHost(): string {
  if (typeof localStorage !== "undefined") {
    return localStorage.getItem(STORAGE_KEY) ?? "maslow.local";
  }
  return "maslow.local";
}

export const connection = writable<ConnectionInfo>({
  host: loadHost(),
  state: "disconnected",
  info: "",
  error: "",
});

export function persistHost(host: string): void {
  if (typeof localStorage !== "undefined") {
    localStorage.setItem(STORAGE_KEY, host);
  }
}

/** Firmware version parsed from [ESP800] on connect, or null. Shown in chrome. */
export const fwVersion = writable<string | null>(null);

/** Connect the websocket and best-effort fetch the firmware version. Shared by
 * the desktop topbar and the mobile connection sheet so the logic lives once. */
export async function connectWs(host: string): Promise<void> {
  connection.update((c) => ({ ...c, host, error: "" }));
  try {
    // Rejects a malformed host before any reconnect loop is started.
    await invoke("connect_ws", { host });
  } catch (e) {
    connection.update((c) => ({ ...c, state: "disconnected", error: String(e) }));
    return;
  }
  persistHost(host);
  try {
    fwVersion.set(await invoke<string | null>("firmware_version", { host }));
  } catch {
    fwVersion.set(null);
  }
}

export async function disconnectWs(): Promise<void> {
  await invoke("disconnect_ws");
  fwVersion.set(null);
}
