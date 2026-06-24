import { writable } from "svelte/store";
import { listen } from "@tauri-apps/api/event";

export interface MachineStatus {
  state: string;
  substate: number | null;
  mpos: number[];
  wpos: number[];
  wco: number[];
  feed: number;
  spindle: number;
  buffer_blocks: number | null;
  buffer_bytes: number | null;
  ov: number[];
}

export type WsState = "connected" | "disconnected";

export const machineStatus = writable<MachineStatus | null>(null);
export const wsState = writable<WsState>("disconnected");
export const consoleLines = writable<string[]>([]);

const MAX_CONSOLE_LINES = 1000;

let started = false;

/** Register Tauri event listeners once for the whole app session. */
export async function initMachineListeners(): Promise<void> {
  if (started) return;
  started = true;

  await listen<MachineStatus>("machine-status", (e) => machineStatus.set(e.payload));

  await listen<string>("ws-state", (e) => {
    wsState.set(e.payload as WsState);
    if (e.payload === "disconnected") machineStatus.set(null);
  });

  await listen<string>("grbl-line", (e) => {
    const line = e.payload;
    // Status reports are surfaced via machine-status; skip them in the console.
    if (line.startsWith("<")) return;
    consoleLines.update((lines) => {
      lines.push(line);
      return lines.length > MAX_CONSOLE_LINES ? lines.slice(-MAX_CONSOLE_LINES) : lines;
    });
  });

  await listen<string>("ws-error", (e) => {
    consoleLines.update((lines) => {
      lines.push(`[ws error] ${e.payload}`);
      return lines.length > MAX_CONSOLE_LINES ? lines.slice(-MAX_CONSOLE_LINES) : lines;
    });
  });
}

export function clearConsole(): void {
  consoleLines.set([]);
}
