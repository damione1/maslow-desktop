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

/** Per-state action policy, computed by the Rust state machine (single source
 * of truth, derived from the firmware's requestStateChange guards). */
export interface StatePolicy {
  code: number;
  label: string;
  busy: boolean;
  allowed: string[];
}

export interface Waypoint {
  n: number;
  x: number;
  y: number;
}

/** Latest MINFO telemetry, or null until first poll. */
export const maslowInfo = writable<MaslowInfo | null>(null);
/** Current calibration state policy, or null until known. */
export const maslowState = writable<StatePolicy | null>(null);
/** Calibration grid waypoints reported during a run. */
export const waypoints = writable<Waypoint[]>([]);
/** Pulses true briefly when a calibration-complete message arrives. */
export const calComplete = writable(false);

let started = false;

export async function initMaslowListeners(): Promise<void> {
  if (started) return;
  started = true;

  await listen<MaslowInfo>("maslow-info", (e) => maslowInfo.set(e.payload));

  await listen<StatePolicy>("maslow-state", (e) => {
    const policy = e.payload;
    // Entering calibration starts a fresh waypoint run.
    if (policy.code === 6) waypoints.set([]);
    maslowState.set(policy);
  });

  await listen<Waypoint>("maslow-waypoint", (e) => {
    waypoints.update((list) => {
      // A lower index than the last means a new run started.
      if (list.length && e.payload.n <= list[list.length - 1].n) {
        return [e.payload];
      }
      return [...list, e.payload];
    });
  });

  await listen("maslow-cal-complete", () => {
    calComplete.set(true);
    setTimeout(() => calComplete.set(false), 6000);
  });

  await listen<string>("ws-state", (e) => {
    if (e.payload === "disconnected") {
      maslowInfo.set(null);
      maslowState.set(null);
    }
  });
}
