// FluidNC's numeric error codes and messages, mirroring firmware Error.h /
// Error.cpp (ErrorNames). The firmware only ever sends the bare code over the
// wire (e.g. "error:3"); this table exists so the console can show the
// operator what it actually means instead of a bare number to look up.
const GRBL_ERRORS: Record<number, string> = {
  1: "Expected GCode command letter",
  2: "Bad GCode number format",
  3: "Invalid $ statement",
  4: "Negative value",
  5: "Setting disabled",
  6: "Step pulse too short",
  7: "Failed to read settings",
  8: "Command requires idle state",
  9: "GCode cannot be executed in lock or alarm state",
  10: "Soft limit error",
  11: "Line too long",
  12: "Max step rate exceeded",
  13: "Check door",
  14: "Startup line too long",
  15: "Max travel exceeded during jog",
  16: "Invalid jog command",
  17: "Laser mode requires PWM output",
  18: "No homing/cycle defined in settings",
  19: "Single axis homing not allowed",
  20: "Unsupported GCode command",
  21: "GCode modal group violation",
  22: "GCode undefined feed rate",
  23: "GCode command value not integer",
  24: "GCode axis command conflict",
  25: "GCode word repeated",
  26: "GCode no axis words",
  27: "GCode invalid line number",
  28: "GCode value word missing",
  29: "GCode unsupported coordinate system",
  30: "GCode G53 invalid motion mode",
  31: "GCode extra axis words",
  32: "GCode no axis words in plane",
  33: "GCode invalid target",
  34: "GCode arc radius error",
  35: "GCode no offsets in plane",
  36: "GCode unused words",
  37: "GCode G43 dynamic axis error",
  38: "GCode max value exceeded",
  39: "P param max exceeded",
  40: "Check control pins",
  60: "Failed to mount device",
  61: "Read failed",
  62: "Failed to open directory",
  63: "Directory not found",
  64: "File empty",
  65: "File not found",
  66: "Failed to open file",
  67: "Device is busy",
  68: "Failed to delete directory",
  69: "Failed to delete file",
  80: "Number out of range for setting",
  81: "Invalid value for setting",
  82: "Failed to create file",
  83: "Failed to format filesystem",
  90: "Failed to send message",
  100: "Failed to store setting",
  101: "Failed to get setting status",
  110: "Authentication failed",
  111: "End of line",
  112: "End of file",
  120: "Another interface is busy",
  130: "Jog cancelled",
  150: "Bad pin specification",
  151: "Bad runtime config setting",
  152: "Configuration is invalid, check boot messages for errors",
  160: "File upload failed",
  161: "File download failed",
};

/** Look up the human-readable message for a numeric GRBL/FluidNC error code. */
export function grblErrorMessage(code: number): string | null {
  return GRBL_ERRORS[code] ?? null;
}

/**
 * If `line` is a bare `error:N` acknowledgement, return it annotated with the
 * firmware's message for that code (e.g. `error:3 (Invalid $ statement)`).
 * Any other line (including already-descriptive `[MSG:...]`/ALARM lines) is
 * returned unchanged.
 */
export function annotateErrorLine(line: string): string {
  const m = line.trim().match(/^error:(\d+)$/i);
  if (!m) return line;
  const code = Number(m[1]);
  const message = grblErrorMessage(code);
  return message ? `${line} (${message})` : line;
}
