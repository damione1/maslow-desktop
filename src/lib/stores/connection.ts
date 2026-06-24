import { writable } from "svelte/store";

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
