// Read access to the machine configuration by firmware path.
//
// The config is discovered from the machine (the `$CD` dump, flattened to
// `ConfigEntry[]` in `fullConfig`), never hard coded. These helpers and the
// `CFG` path constants let the rest of the app read specific settings by their
// firmware path instead of through a parallel typed struct, so adding or
// removing a firmware key never requires a matching code change here.

import type { ConfigEntry } from "$lib/stores/maslow";

/** Firmware config paths the app reads or writes by name. Values are the exact
 * `$/<path>` keys the firmware exposes (root `Maslow_*` items and the
 * `kinematics/MaslowKinematics/<coord>` anchors). */
export const CFG = {
  // Work area
  workAreaX: "Maslow_Work_Area_X",
  workAreaY: "Maslow_Work_Area_Y",
  workAreaCenterOffsetX: "Maslow_Work_Area_Center_Offset_X",
  workAreaCenterOffsetY: "Maslow_Work_Area_Center_Offset_Y",
  // Belt tension / extension
  retractCurrentThreshold: "Maslow_Retract_Current_Threshold",
  extendDist: "Maslow_Extend_Dist",
  applyTensionBeltRetractionLimit: "Maslow_Apply_Tension_Belt_Retraction_Limit",
  applyTensionAllowLimiting: "Maslow_Apply_Tension_Allow_Limiting",
  // Material
  spoilboardThickness: "Maslow_spoilboardThickness",
  workThickness: "Maslow_workThickness",
  // Calibration
  calibrationGridSize: "Maslow_calibration_grid_size",
  calibrationGridWidthX: "Maslow_calibration_grid_width_mm_X",
  calibrationGridHeightY: "Maslow_calibration_grid_height_mm_Y",
  acceptableCalibrationThreshold: "Maslow_Acceptable_Calibration_Threshold",
  scaleX: "Maslow_Scale_X",
  scaleY: "Maslow_Scale_Y",
  vertical: "Maslow_vertical",
  // Park position
  parkX: "Maslow_Park_X",
  parkY: "Maslow_Park_Y",
  parkZ: "Maslow_Park_Z",
} as const;

/** Find a config entry by exact firmware path. */
export function configEntry(
  entries: ConfigEntry[] | null | undefined,
  path: string,
): ConfigEntry | undefined {
  return entries?.find((e) => e.path === path);
}

/** Read a numeric config value by path, falling back when absent or unparsable. */
export function configNumber(
  entries: ConfigEntry[] | null | undefined,
  path: string,
  fallback: number,
): number {
  const e = configEntry(entries, path);
  if (!e) return fallback;
  const n = Number(e.value);
  return Number.isFinite(n) ? n : fallback;
}

/** Read a boolean config value by path (firmware spells these true/1/yes/on). */
export function configBool(
  entries: ConfigEntry[] | null | undefined,
  path: string,
  fallback: boolean,
): boolean {
  const e = configEntry(entries, path);
  if (!e) return fallback;
  const v = e.value.trim().toLowerCase();
  return v === "true" || v === "1" || v === "yes" || v === "on";
}
