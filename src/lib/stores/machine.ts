import { writable } from "svelte/store";
import { listen } from "@tauri-apps/api/event";
import { annotateErrorLine } from "$lib/stores/grblErrors";

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
    // Bare `ok` acks are noise (telemetry polling emits them ~every 1.5s).
    if (line === "ok") return;
    // A bare `error:N` (from a poll or another out-of-band command, not
    // necessarily the job) is meaningless without looking up the code, so
    // annotate it with the firmware's own message for it.
    const annotated = annotateErrorLine(line);
    consoleLines.update((lines) => {
      lines.push(annotated);
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

/** Map a FluidNC machine state to a color class (shared by StatusPanel and the
 * global top-bar state pill so they stay in sync). */
export function stateClass(state: string): string {
  switch (state) {
    case "Idle":
      return "idle";
    case "Run":
    case "Jog":
    case "Home":
      return "run";
    case "Hold":
    case "Door":
      return "hold";
    case "Alarm":
      return "alarm";
    default:
      return "other";
  }
}

/** Append a synthetic line to the console (used for diagnostics like state
 * discordances). */
export function pushConsoleLine(line: string): void {
  consoleLines.update((lines) => {
    lines.push(line);
    return lines.length > MAX_CONSOLE_LINES ? lines.slice(-MAX_CONSOLE_LINES) : lines;
  });
}
