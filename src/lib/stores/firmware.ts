import { derived } from "svelte/store";
import { fwVersion } from "$lib/stores/connection";

// The Maslow FluidNC firmware range this build has actually been tested against.
// Keep these in sync with the README and the release notes. When you validate a
// newer firmware, bump SUPPORTED_MAX (and ship the matching fixes).
export const SUPPORTED_MIN = "1.21";
export const SUPPORTED_MAX = "1.22";

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

/** A warning message when the connected firmware is outside the tested range,
 * or null when it is in range / unknown. Drives the chrome warning banner. */
export const firmwareNotice = derived(fwVersion, ($ver) => {
  const support = firmwareSupport($ver);
  if (support === "ok" || support === "unknown") return null;
  const direction = support === "untested_old" ? "older than" : "newer than";
  return (
    `Firmware ${$ver} is ${direction} the tested range ` +
    `(FluidNC v${SUPPORTED_MIN} to v${SUPPORTED_MAX}). ` +
    `This software has not been tested with it; some actions may not behave as expected. Proceed with caution.`
  );
});
