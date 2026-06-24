import { get, writable } from "svelte/store";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { pushConsoleLine, wsState } from "$lib/stores/machine";
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
  apply_tension_belt_retraction_limit: number;
  apply_tension_allow_limiting: boolean;
  spoilboard_thickness: number;
  work_thickness: number;
  calibration_grid_size: number;
  calibration_grid_width_x: number;
  calibration_grid_height_y: number;
  acceptable_calibration_threshold: number;
  scale_x: number;
  scale_y: number;
  vertical: boolean;
  park_x: number;
  park_y: number;
  park_z: number;
  anchors_valid: boolean;
}

interface Discord {
  kind: string;
  from: number;
  to: number;
  from_label: string;
  to_label: string;
}

/** One editable leaf of the full FluidNC config tree (`$CD` flattened). `path`
 * is the `/`-joined key that `$/<path>=<value>` expects. */
export interface ConfigEntry {
  path: string;
  value: string;
  kind: "bool" | "int" | "float" | "text";
}

/** One raw belt-length measurement at a calibration waypoint (mm). */
export interface Measurement {
  tl: number;
  tr: number;
  bl: number;
  br: number;
}

/** Quality metrics of a solved fit (mirrors Rust Fitness / firmware). */
export interface Fitness {
  rms: number;
  max_residual: number;
  per_anchor: [number, number, number, number];
  converged: boolean;
}

/** Fitness numbers the firmware logs after its own recompute. */
export interface FirmwareFit {
  rms: number;
  max_residual: number;
  per_anchor: [number, number, number, number];
  converged: boolean;
}

/** Reduced 5-parameter anchor estimate (BL at origin, brY=0). */
export interface AnchorParams {
  tl_x: number;
  tl_y: number;
  tr_x: number;
  tr_y: number;
  br_x: number;
}

/** Result of a client-side solve (mirrors Rust SolveResult). */
export interface SolveResult {
  solver: string;
  ok: boolean;
  anchors: Anchors;
  params: AnchorParams;
  fitness: Fitness;
  sled: { x: number; y: number }[];
  residuals: [number, number, number, number][];
  kept_indices: number[];
  gate_error: string | null;
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

/** Raw belt measurements from the last `CLBM:` log, input to the local solver. */
export const measurements = writable<Measurement[]>([]);
/** The firmware's own fit metrics from the last recompute (for comparison). */
export const firmwareFit = writable<FirmwareFit | null>(null);
/** The anchors the firmware computed on-device (for comparison). */
export const firmwareAnchors = writable<AnchorParams | null>(null);
/** Result of the last client-side solve, or null. */
export const localSolve = writable<SolveResult | null>(null);
/** Original waypoint indices excluded from the next solve (what-if). */
export const excludedWaypoints = writable<Set<number>>(new Set());

/** Run the client-side solver over the current measurements, excluding the
 * given waypoints, seeded from the current config anchors. Pure compute. */
export async function solveLocally(): Promise<void> {
  const ms = get(measurements);
  if (!ms.length) return;
  const exclude = [...get(excludedWaypoints)];
  const initial = get(anchors); // null → Rust falls back to firmware defaults
  const result = await invoke<SolveResult>("solve_calibration", {
    measurements: ms,
    initial,
    exclude,
    solver: "levenberg-marquardt",
  });
  localSolve.set(result);
}

/** Toggle a waypoint in/out of the exclusion set without re-solving. */
export function toggleExcluded(index: number): void {
  excludedWaypoints.update((set) => {
    const next = new Set(set);
    if (next.has(index)) next.delete(index);
    else next.add(index);
    return next;
  });
}

/** The full FluidNC config tree (every editable leaf), or null until read. */
export const fullConfig = writable<ConfigEntry[] | null>(null);

/** Request a `$CD` config dump over the WS. The HTTP `/command` endpoint
 * returns an empty body for `$`/`$/` commands (the firmware routes the output
 * to the active WS channel instead), so reads must go over the socket. One dump
 * fills `fullConfig`, `maslowConfig` and `anchors` via the events below. */
export async function requestConfigDump(): Promise<void> {
  if (get(wsState) !== "connected") return;
  await invoke("request_config_dump");
}

// The three panels each have a "Read" button; all of them trigger the same
// single dump, and the stores fill asynchronously from the dump events.
export const refreshFullConfig = requestConfigDump;
export const refreshConfig = requestConfigDump;
export const refreshAnchors = requestConfigDump;

let started = false;

export async function initMaslowListeners(): Promise<void> {
  if (started) return;
  started = true;

  await listen<MaslowInfo>("maslow-info", (e) => maslowInfo.set(e.payload));

  await listen<StatePolicy>("maslow-state", (e) => {
    const policy = e.payload;
    // Entering calibration starts a fresh waypoint run, and invalidates any
    // measurements/solve from a previous run.
    if (policy.code === 6) {
      waypoints.set([]);
      measurements.set([]);
      firmwareFit.set(null);
      firmwareAnchors.set(null);
      localSolve.set(null);
      excludedWaypoints.set(new Set());
    }
    maslowState.set(policy);
  });

  await listen<Measurement[]>("cal-measurements", (e) => {
    measurements.set(e.payload);
  });
  await listen<FirmwareFit>("cal-firmware-fit", (e) => firmwareFit.set(e.payload));
  await listen<AnchorParams>("cal-firmware-anchors", (e) =>
    firmwareAnchors.set(e.payload),
  );

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

  // A single `$CD` dump (requestConfigDump) fills all three config stores.
  await listen<ConfigEntry[]>("config-dump", (e) => fullConfig.set(e.payload));
  await listen<MaslowConfig>("maslow-config", (e) => maslowConfig.set(e.payload));
  await listen<Anchors>("maslow-anchors", (e) => anchors.set(e.payload));
  await listen<string>("config-dump-error", (e) =>
    pushConsoleLine(`[config dump] ${e.payload}`),
  );

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
      fullConfig.set(null);
      measurements.set([]);
      firmwareFit.set(null);
      firmwareAnchors.set(null);
      localSolve.set(null);
      excludedWaypoints.set(new Set());
    } else if (e.payload === "connected") {
      // Learn the calibration status as soon as we are live.
      refreshAnchors();
    }
  });
}
