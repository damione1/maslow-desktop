import { derived } from "svelte/store";
import { fwVersion } from "$lib/stores/connection";

// The Maslow FluidNC firmware range this build has actually been tested against.
// Keep these in sync with the README and the release notes. When you validate a
// newer firmware, bump SUPPORTED_MAX (and ship the matching fixes).
export const SUPPORTED_MIN = "1.21";
export const SUPPORTED_MAX = "1.22";

// Firmware version at which full calibration became usable through this app. Below this,
// the firmware drives calibration through the `$ACKCAL` client-recompute handshake (removed
// in 1.22, where recompute moved on-device); this app does not implement that handshake, so
// calibration must be run from the firmware's embedded web UI on older builds instead.
const CALIBRATION_MIN = "1.22";

export type FwSupport = "ok" | "untested_old" | "untested_new" | "unknown";

/** Extract [major, minor] from a version string like "1.22" or "1.22-3-gabc". */
function majorMinor(ver: string): [number, number] | null {
  const m = ver.match(/(\d+)\.(\d+)/);
  return m ? [Number(m[1]), Number(m[2])] : null;
}

function cmp(a: [number, number], b: [number, number]): number {
  return a[0] !== b[0] ? a[0] - b[0] : a[1] - b[1];
}

/** Classify the connected firmware against the tested range (major.minor). */
export function firmwareSupport(ver: string | null | undefined): FwSupport {
  if (!ver) return "unknown";
  const v = majorMinor(ver);
  if (!v) return "unknown";
  if (cmp(v, majorMinor(SUPPORTED_MIN)!) < 0) return "untested_old";
  if (cmp(v, majorMinor(SUPPORTED_MAX)!) > 0) return "untested_new";
  return "ok";
}

/** Whether this app's calibration wizard can run a full calibration on the connected
 * firmware. Unknown versions are treated as supported so an unreadable version string
 * doesn't needlessly block the operator. */
export function supportsFullCalibration(ver: string | null | undefined): boolean {
  if (!ver) return true;
  const v = majorMinor(ver);
  if (!v) return true;
  return cmp(v, majorMinor(CALIBRATION_MIN)!) >= 0;
}

/** A warning message when the connected firmware is outside the tested range,
 * or null when it is in range / unknown. Drives the chrome warning banner. */
export const firmwareNotice = derived(fwVersion, ($ver) => {
  const support = firmwareSupport($ver);
  if (support === "ok" || support === "unknown") return null;
  const direction = support === "untested_old" ? "older than" : "newer than";
  // Firmware below the tested range is always below CALIBRATION_MIN too, so
  // it isn't just "untested": full calibration through this app specifically
  // won't work there. Say that plainly instead of folding it into the
  // generic "some actions may not behave as expected" caveat.
  const calibrationCaveat =
    support === "untested_old"
      ? ` Full calibration specifically requires firmware v${CALIBRATION_MIN}+; use the firmware's embedded web UI for that on this machine.`
      : "";
  return (
    `Firmware ${$ver} is ${direction} the tested range ` +
    `(FluidNC v${SUPPORTED_MIN} to v${SUPPORTED_MAX}).` +
    `${calibrationCaveat} ` +
    `This software has not been tested with it; some other actions may not behave as expected. Proceed with caution.`
  );
});
