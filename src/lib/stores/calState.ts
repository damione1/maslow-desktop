// Maslow calibration state codes (mirrors the firmware `MaslowEnums.h` and the
// Rust `CalState` enum). The firmware owns this state and reports it as an
// integer; this module gives the frontend named constants instead of magic
// numbers, plus a few shared predicates.

export const CalState = {
  Unknown: 0,
  Retracting: 1,
  Retracted: 2,
  Extending: 3,
  ExtendedOut: 4,
  TakingSlack: 5,
  CalibrationInProgress: 6,
  ReadyToCut: 7,
  ReleaseTension: 8,
  CalibrationComputing: 9,
} as const;

export type CalStateCode = (typeof CalState)[keyof typeof CalState];

/** The machine is tensioned and ready to stream a cut. */
export function isReadyToCut(code: number | null | undefined): boolean {
  return code === CalState.ReadyToCut;
}

/** A stable pre-cut state from which the daily "resume" path can apply tension
 * (EXTENDEDOUT) or extend then tension (RETRACTED) without a full recalibration. */
export function isResumablePreCut(code: number | null | undefined): boolean {
  return code === CalState.ExtendedOut || code === CalState.Retracted;
}
