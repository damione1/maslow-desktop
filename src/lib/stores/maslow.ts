import { writable } from "svelte/store";
import { listen } from "@tauri-apps/api/event";

export interface MaslowInfo {
  homed: boolean;
  calibrationInProgress: boolean;
  tl: number;
  tr: number;
  br: number;
  bl: number;
  etl: number;
  etr: number;
  ebr: number;
  ebl: number;
  extended: boolean;
}

/** Latest MINFO telemetry, or null until first poll. */
export const maslowInfo = writable<MaslowInfo | null>(null);
/** Calibration state machine code (0-9), or null until known. */
export const maslowState = writable<number | null>(null);

export const STATE_NAMES: Record<number, string> = {
  0: "Unknown",
  1: "Retracting",
  2: "Retracted",
  3: "Extending",
  4: "Extended",
  5: "Taking Slack",
  6: "Calibrating",
  7: "Ready to Cut",
  8: "Releasing Tension",
  9: "Computing",
};

/** States where a belt/calibration operation is actively running. */
export const BUSY_STATES = new Set([1, 3, 5, 6, 9]);

let started = false;

export async function initMaslowListeners(): Promise<void> {
  if (started) return;
  started = true;
  await listen<MaslowInfo>("maslow-info", (e) => maslowInfo.set(e.payload));
  await listen<number>("maslow-state", (e) => maslowState.set(e.payload));
  await listen<string>("ws-state", (e) => {
    if (e.payload === "disconnected") {
      maslowInfo.set(null);
      maslowState.set(null);
    }
  });
}
