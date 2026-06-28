// Presentation metadata for machine config fields, keyed by firmware path.
//
// This is the ONLY hand maintained piece of the config system, and it is purely
// additive: it enriches fields that the machine reports (label, help, bounds,
// grouping), but it never decides whether a field exists. A discovered path
// with no descriptor still renders generically, so a new firmware key shows up
// on its own and a removed key simply disappears. Maintaining a descriptor is a
// choice for nicer UX, not a requirement for correctness (the firmware is the
// real backstop: it rejects out of range writes).

export type Widget = "number" | "bool" | "text";

export interface ConfigDescriptor {
  /** Exact path, a prefix ending in "/", or a "*" suffix wildcard. */
  match: string;
  label?: string;
  group?: string;
  help?: string;
  min?: number;
  max?: number;
  step?: number;
  widget?: Widget;
  /** Hide from curated views (still visible in the full tree). */
  hidden?: boolean;
  /** Collapse behind an "advanced" disclosure in curated views. */
  advanced?: boolean;
}

/** Resolved metadata for one path (descriptor merged with fallbacks). */
export interface ResolvedField {
  path: string;
  label: string;
  group?: string;
  help?: string;
  min?: number;
  max?: number;
  step?: number;
  widget?: Widget;
  hidden: boolean;
  advanced: boolean;
}

const KIN = "kinematics/MaslowKinematics";
const ANCHOR_HELP =
  "Frame anchor coordinate (mm). Normally set automatically by calibration; edit by hand only if you know the measured frame geometry.";

function anchor(coord: string, label: string, z = false): ConfigDescriptor {
  return {
    match: `${KIN}/${coord}`,
    label,
    group: "Frame anchors",
    help: ANCHOR_HELP,
    min: z ? 0 : -500,
    max: z ? 500 : 10000,
    step: 0.1,
    advanced: true,
  };
}

// Bounds mirror the firmware (MachineConfig::groupM4Items ranges and the
// MaslowKinematics validation). The firmware does not clamp runtime float
// writes, so these client bounds are the practical guard for floats.
export const DESCRIPTORS: ConfigDescriptor[] = [
  // Frame anchors (advanced)
  anchor("tlX", "TL X"), anchor("tlY", "TL Y"), anchor("tlZ", "TL Z", true),
  anchor("trX", "TR X"), anchor("trY", "TR Y"), anchor("trZ", "TR Z", true),
  anchor("blX", "BL X"), anchor("blY", "BL Y"), anchor("blZ", "BL Z", true),
  anchor("brX", "BR X"), anchor("brY", "BR Y"), anchor("brZ", "BR Z", true),
  // Work area
  { match: "Maslow_Work_Area_X", label: "Width X", group: "Work area", min: 1, max: 10000, step: 1, help: "Width of the usable cutting area (mm)." },
  { match: "Maslow_Work_Area_Y", label: "Height Y", group: "Work area", min: 1, max: 10000, step: 1, help: "Height of the usable cutting area (mm)." },
  { match: "Maslow_Work_Area_Center_Offset_X", label: "Center offset X", group: "Work area", min: -5000, max: 5000, step: 1, help: "Horizontal shift of the work-area centre from the frame centre (mm)." },
  { match: "Maslow_Work_Area_Center_Offset_Y", label: "Center offset Y", group: "Work area", min: -5000, max: 5000, step: 1, help: "Vertical shift of the work-area centre from the frame centre (mm)." },
  // Belt tension / extension
  { match: "Maslow_Retract_Current_Threshold", label: "Retract current threshold", group: "Belt tension", min: 0, max: 3500, step: 50, help: "Motor current at which a belt is considered fully tight. Higher = tighter belts (and more strain)." },
  { match: "Maslow_Extend_Dist", label: "Extend distance", group: "Belt tension", min: 0, max: 4250, step: 10, help: "How far the belts pay out on Extend before calibration (mm). Set automatically after Find Anchors." },
  { match: "Maslow_Apply_Tension_Belt_Retraction_Limit", label: "Apply-tension retraction limit", group: "Belt tension", min: 0, max: 4250, step: 10, help: "Maximum belt retraction allowed while applying tension (mm). Firmware v1.22 or newer." },
  { match: "Maslow_Apply_Tension_Allow_Limiting", label: "Allow tension limiting", group: "Belt tension", widget: "bool", help: "Allow the apply-tension step to cap belt retraction at the limit above. Firmware v1.22 or newer." },
  // Material
  { match: "Maslow_spoilboardThickness", label: "Spoilboard thickness", group: "Material", min: 0, max: 50, step: 0.1, help: "Spoilboard thickness (mm). Offsets Z so calibration and Z-zero account for it." },
  { match: "Maslow_workThickness", label: "Work thickness", group: "Material", min: 0, max: 50, step: 0.1, help: "Workpiece thickness (mm). Added to the Z offset during calibration." },
  // Calibration
  { match: "Maslow_calibration_grid_size", label: "Grid size", group: "Calibration", min: 3, max: 9, step: 2, help: "Measurement points per side of the calibration grid (3, 5, 7 or 9). More = more accurate but slower." },
  { match: "Maslow_calibration_grid_width_mm_X", label: "Grid width X", group: "Calibration", min: 0, max: 3000, step: 1, help: "Width of the calibration grid (mm). 0 = derive from the work area." },
  { match: "Maslow_calibration_grid_height_mm_Y", label: "Grid height Y", group: "Calibration", min: 0, max: 3000, step: 1, help: "Height of the calibration grid (mm). 0 = derive from the work area." },
  { match: "Maslow_Acceptable_Calibration_Threshold", label: "Acceptable fitness", group: "Calibration", min: 0, max: 1, step: 0.01, help: "Fit error (mm) below which a calibration pass is accepted. Lower = stricter." },
  { match: "Maslow_Scale_X", label: "Scale X", group: "Calibration", min: 0.8, max: 1.2, step: 0.001, help: "Linear scale correction on X (about 1.0). Compensates small dimensional error." },
  { match: "Maslow_Scale_Y", label: "Scale Y", group: "Calibration", min: 0.8, max: 1.2, step: 0.001, help: "Linear scale correction on Y (about 1.0). Compensates small dimensional error." },
  { match: "Maslow_vertical", label: "Vertical orientation", group: "Calibration", widget: "bool", help: "On if the machine frame hangs vertically (the usual Maslow setup)." },
  // Park position
  { match: "Maslow_Park_X", label: "Park X", group: "Park", min: -10000, max: 10000, step: 1, help: "Park position X in machine coordinates (mm)." },
  { match: "Maslow_Park_Y", label: "Park Y", group: "Park", min: -10000, max: 10000, step: 1, help: "Park position Y in machine coordinates (mm)." },
  { match: "Maslow_Park_Z", label: "Park Z", group: "Park", min: -100, max: 100, step: 0.1, help: "Z lift in work coordinates when parking (mm)." },
];

function specificity(match: string): number {
  if (match.endsWith("*")) return match.length - 1; // wildcard, by prefix length
  if (match.endsWith("/")) return match.length; // prefix
  return 1000 + match.length; // exact beats any prefix
}

function matches(match: string, path: string): boolean {
  if (match.endsWith("*")) return path.startsWith(match.slice(0, -1));
  if (match.endsWith("/")) return path.startsWith(match);
  return match === path;
}

const leaf = (path: string) => {
  const i = path.lastIndexOf("/");
  return i === -1 ? path : path.slice(i + 1);
};

/** Resolve presentation metadata for a path: the most specific matching
 * descriptor wins; unknown paths get a generic fallback (label = trailing
 * segment, no bounds). */
export function describe(path: string): ResolvedField {
  let best: ConfigDescriptor | undefined;
  let bestScore = -1;
  for (const d of DESCRIPTORS) {
    if (matches(d.match, path)) {
      const score = specificity(d.match);
      if (score > bestScore) {
        best = d;
        bestScore = score;
      }
    }
  }
  return {
    path,
    label: best?.label ?? leaf(path),
    group: best?.group,
    help: best?.help,
    min: best?.min,
    max: best?.max,
    step: best?.step,
    widget: best?.widget,
    hidden: best?.hidden ?? false,
    advanced: best?.advanced ?? false,
  };
}
