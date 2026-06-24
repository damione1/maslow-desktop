import { get, writable } from "svelte/store";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { pushConsoleLine } from "$lib/stores/machine";
import { connection } from "$lib/stores/connection";

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

/** Unified action policy reconciling the FluidNC state, the Maslow calibration
 * state and any running job. Computed in Rust (single source of truth). */
export interface ActionPolicy {
  jog: boolean;
  home: boolean;
  unlock: boolean;
  zero: boolean;
  run: boolean;
  hold: boolean;
  resume: boolean;
  reset: boolean;
  retract: boolean;
  extend: boolean;
  take_slack: boolean;
  calibrate: boolean;
  comply: boolean;
  stop: boolean;
  estop: boolean;
}

/** Frame anchor coordinates read from the firmware config, with a `valid` flag
 * computed in Rust (anchors usable → no recalibration needed). */
export interface Anchors {
  tl_x: number;
  tl_y: number;
  tr_x: number;
  tr_y: number;
  bl_x: number;
  bl_y: number;
  br_x: number;
  br_y: number;
  valid: boolean;
}

/** Full Maslow firmware configuration (anchors + work area + tension),
 * read/written via `$/<key>`. Field names map to the FluidNC config keys. */
export interface MaslowConfig {
  tl_x: number;
  tl_y: number;
  tl_z: number;
  tr_x: number;
  tr_y: number;
  tr_z: number;
  bl_x: number;
  bl_y: number;
  bl_z: number;
  br_x: number;
  br_y: number;
  br_z: number;
  work_area_x: number;
  work_area_y: number;
  work_area_center_offset_x: number;
  work_area_center_offset_y: number;
  retract_current_threshold: number;
  extend_dist: number;
  anchors_valid: boolean;
}

interface Discord {
  kind: string;
  from: number;
  to: number;
  from_label: string;
  to_label: string;
}

/** Latest MINFO telemetry, or null until first poll. */
export const maslowInfo = writable<MaslowInfo | null>(null);
/** Current calibration state policy, or null until known. */
export const maslowState = writable<StatePolicy | null>(null);
/** Calibration grid waypoints reported during a run. */
export const waypoints = writable<Waypoint[]>([]);
/** Pulses true briefly when a calibration-complete message arrives. */
export const calComplete = writable(false);
/** Allowed actions for the current combined state, or null when disconnected. */
export const actionPolicy = writable<ActionPolicy | null>(null);
/** Frame anchors read from the firmware config, or null until first read. */
export const anchors = writable<Anchors | null>(null);
/** Full editable Maslow config, or null until first read of the config screen. */
export const maslowConfig = writable<MaslowConfig | null>(null);

/** Read the full Maslow configuration (anchors + work area + tension) from the
 * firmware over HTTP. Heavier than `refreshAnchors` (several `$/` reads), so it
 * is only triggered by the config screen, not on every connect. */
export async function refreshConfig(): Promise<void> {
  const host = get(connection).host;
  if (!host) return;
  maslowConfig.set(await invoke<MaslowConfig>("read_maslow_config", { host }));
}

/** Fetch the frame anchors from the firmware (HTTP) to learn whether the
 * machine is already calibrated. Safe to call repeatedly; failures are
 * swallowed (the badge simply stays unknown). */
export async function refreshAnchors(): Promise<void> {
  const host = get(connection).host;
  if (!host) return;
  try {
    anchors.set(await invoke<Anchors>("read_maslow_anchors", { host }));
  } catch {
    anchors.set(null);
  }
}

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
    // Anchors were just (re)written to the config — refresh the badge.
    refreshAnchors();
  });

  await listen<ActionPolicy>("action-policy", (e) => actionPolicy.set(e.payload));

  // Log state discordances so capricious firmware reports can be identified.
  await listen<Discord>("maslow-discord", (e) => {
    const d = e.payload;
    const tag =
      d.kind === "straggler"
        ? "[state straggler ignored]"
        : "[state discord — machine prevailed]";
    pushConsoleLine(
      `${tag} ${d.from}:${d.from_label} → ${d.to}:${d.to_label}`,
    );
  });

  await listen<string>("ws-state", (e) => {
    if (e.payload === "disconnected") {
      maslowInfo.set(null);
      maslowState.set(null);
      actionPolicy.set(null);
      anchors.set(null);
      maslowConfig.set(null);
    } else if (e.payload === "connected") {
      // Learn the calibration status as soon as we are live.
      refreshAnchors();
    }
  });
}
