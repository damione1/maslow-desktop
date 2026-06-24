<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { wsState } from "$lib/stores/machine";
  import { connection } from "$lib/stores/connection";
  import { jobProgress } from "$lib/stores/job";
  import { maslowConfig, refreshConfig, type MaslowConfig } from "$lib/stores/maslow";

  // Maslow configuration screen. Reads the firmware settings (anchors + work
  // area + belt tension) via `$/<key>` and writes them back with
  // `$/<key>=<value>` followed by `$CO` to persist to flash. The firmware owns
  // the values; this is a thin, validated editor. Editing the anchors directly
  // is an advanced action (normally done by calibration), so it sits behind an
  // explicit toggle and a geometry sanity gate mirroring the firmware's
  // checkBoundaries.

  type BoolKey = "vertical" | "apply_tension_allow_limiting";
  type NumKey = Exclude<keyof MaslowConfig, "anchors_valid" | BoolKey>;
  type Group = "anchor" | "work" | "tension" | "material" | "calibration" | "park";

  interface NumField {
    key: NumKey;
    /** Full FluidNC config path used in `$/<path>` and `$/<path>=<value>`. */
    path: string;
    label: string;
    min: number;
    max: number;
    step: number;
    group: Group;
    /** Plain-language explanation shown as a tooltip (firmware semantics). */
    help: string;
  }

  interface BoolField {
    key: BoolKey;
    path: string;
    label: string;
    group: Group;
    help: string;
  }

  const KIN = "kinematics/MaslowKinematics";
  const ANCHOR_HELP =
    "Frame anchor coordinate (mm). Normally set automatically by calibration — edit by hand only if you know the measured frame geometry.";

  // Bounds mirror the firmware (MachineConfig::groupM4Items item() ranges and
  // MaslowKinematics validation). NB: the firmware does NOT clamp runtime float
  // writes, so client-side validation is the real guard for those.
  const FIELDS: NumField[] = [
    // Anchors (mm) — advanced. X/Y allow the slightly-negative real frame
    // values (tlX ≈ -27.6); Z is the anchor height.
    { key: "tl_x", path: `${KIN}/tlX`, label: "TL X", min: -500, max: 10000, step: 0.1, group: "anchor", help: ANCHOR_HELP },
    { key: "tl_y", path: `${KIN}/tlY`, label: "TL Y", min: -500, max: 10000, step: 0.1, group: "anchor", help: ANCHOR_HELP },
    { key: "tl_z", path: `${KIN}/tlZ`, label: "TL Z", min: 0, max: 500, step: 0.1, group: "anchor", help: ANCHOR_HELP },
    { key: "tr_x", path: `${KIN}/trX`, label: "TR X", min: -500, max: 10000, step: 0.1, group: "anchor", help: ANCHOR_HELP },
    { key: "tr_y", path: `${KIN}/trY`, label: "TR Y", min: -500, max: 10000, step: 0.1, group: "anchor", help: ANCHOR_HELP },
    { key: "tr_z", path: `${KIN}/trZ`, label: "TR Z", min: 0, max: 500, step: 0.1, group: "anchor", help: ANCHOR_HELP },
    { key: "bl_x", path: `${KIN}/blX`, label: "BL X", min: -500, max: 10000, step: 0.1, group: "anchor", help: ANCHOR_HELP },
    { key: "bl_y", path: `${KIN}/blY`, label: "BL Y", min: -500, max: 10000, step: 0.1, group: "anchor", help: ANCHOR_HELP },
    { key: "bl_z", path: `${KIN}/blZ`, label: "BL Z", min: 0, max: 500, step: 0.1, group: "anchor", help: ANCHOR_HELP },
    { key: "br_x", path: `${KIN}/brX`, label: "BR X", min: -500, max: 10000, step: 0.1, group: "anchor", help: ANCHOR_HELP },
    { key: "br_y", path: `${KIN}/brY`, label: "BR Y", min: -500, max: 10000, step: 0.1, group: "anchor", help: ANCHOR_HELP },
    { key: "br_z", path: `${KIN}/brZ`, label: "BR Z", min: 0, max: 500, step: 0.1, group: "anchor", help: ANCHOR_HELP },
    // Work area (mm).
    { key: "work_area_x", path: "Maslow_Work_Area_X", label: "Width X", min: 1, max: 10000, step: 1, group: "work", help: "Width of the usable cutting area (mm)." },
    { key: "work_area_y", path: "Maslow_Work_Area_Y", label: "Height Y", min: 1, max: 10000, step: 1, group: "work", help: "Height of the usable cutting area (mm)." },
    { key: "work_area_center_offset_x", path: "Maslow_Work_Area_Center_Offset_X", label: "Center offset X", min: -5000, max: 5000, step: 1, group: "work", help: "Horizontal shift of the work-area centre from the frame centre (mm)." },
    { key: "work_area_center_offset_y", path: "Maslow_Work_Area_Center_Offset_Y", label: "Center offset Y", min: -5000, max: 5000, step: 1, group: "work", help: "Vertical shift of the work-area centre from the frame centre (mm)." },
    // Belt tension / extension.
    { key: "retract_current_threshold", path: "Maslow_Retract_Current_Threshold", label: "Retract current threshold", min: 0, max: 3500, step: 50, group: "tension", help: "Motor current at which a belt is considered fully tight when retracting / taking up slack. Higher = tighter belts (and more strain)." },
    { key: "extend_dist", path: "Maslow_Extend_Dist", label: "Extend distance", min: 0, max: 4250, step: 10, group: "tension", help: "How far the belts pay out when you press Extend, before calibration (mm). Set automatically after Find Anchors." },
    { key: "apply_tension_belt_retraction_limit", path: "Maslow_Apply_Tension_Belt_Retraction_Limit", label: "Apply-tension retraction limit", min: 0, max: 4250, step: 10, group: "tension", help: "Maximum belt retraction allowed while applying tension (mm). Firmware ≥ v1.22." },
    // Material thickness (mm).
    { key: "spoilboard_thickness", path: "Maslow_spoilboardThickness", label: "Spoilboard thickness", min: 0, max: 50, step: 0.1, group: "material", help: "Spoilboard thickness (mm). Offsets Z so calibration and Z-zero account for it." },
    { key: "work_thickness", path: "Maslow_workThickness", label: "Work thickness", min: 0, max: 50, step: 0.1, group: "material", help: "Workpiece thickness (mm). Added to the Z offset during calibration." },
    // Calibration tuning.
    { key: "calibration_grid_size", path: "Maslow_calibration_grid_size", label: "Grid size", min: 3, max: 9, step: 2, group: "calibration", help: "Number of measurement points per side of the calibration grid (3, 5, 7 or 9). More points = more accurate but slower." },
    { key: "calibration_grid_width_x", path: "Maslow_calibration_grid_width_mm_X", label: "Grid width X", min: 0, max: 3000, step: 1, group: "calibration", help: "Width of the calibration measurement grid (mm). 0 = derive from the work area." },
    { key: "calibration_grid_height_y", path: "Maslow_calibration_grid_height_mm_Y", label: "Grid height Y", min: 0, max: 3000, step: 1, group: "calibration", help: "Height of the calibration measurement grid (mm). 0 = derive from the work area." },
    { key: "acceptable_calibration_threshold", path: "Maslow_Acceptable_Calibration_Threshold", label: "Acceptable fitness", min: 0, max: 1, step: 0.01, group: "calibration", help: "Fit error (mm) below which a calibration pass is accepted before measuring more points. Lower = stricter." },
    { key: "scale_x", path: "Maslow_Scale_X", label: "Scale X", min: 0.8, max: 1.2, step: 0.001, group: "calibration", help: "Linear scale correction on the X axis (≈ 1.0). Compensates small dimensional error." },
    { key: "scale_y", path: "Maslow_Scale_Y", label: "Scale Y", min: 0.8, max: 1.2, step: 0.001, group: "calibration", help: "Linear scale correction on the Y axis (≈ 1.0). Compensates small dimensional error." },
    // Park position.
    { key: "park_x", path: "Maslow_Park_X", label: "Park X", min: -10000, max: 10000, step: 1, group: "park", help: "Park position X in machine coordinates (mm)." },
    { key: "park_y", path: "Maslow_Park_Y", label: "Park Y", min: -10000, max: 10000, step: 1, group: "park", help: "Park position Y in machine coordinates (mm)." },
    { key: "park_z", path: "Maslow_Park_Z", label: "Park Z", min: -100, max: 100, step: 0.1, group: "park", help: "Z lift in work coordinates when parking (mm)." },
  ];

  const BOOL_FIELDS: BoolField[] = [
    { key: "vertical", path: "Maslow_vertical", label: "Vertical orientation", group: "calibration", help: "On if the machine frame hangs vertically (the usual Maslow setup)." },
    { key: "apply_tension_allow_limiting", path: "Maslow_Apply_Tension_Allow_Limiting", label: "Allow tension limiting", group: "tension", help: "Allow the apply-tension step to cap belt retraction at the limit above. Firmware ≥ v1.22." },
  ];

  const anchorFields = FIELDS.filter((f) => f.group === "anchor");
  const workFields = FIELDS.filter((f) => f.group === "work");
  const tensionFields = FIELDS.filter((f) => f.group === "tension");
  const materialFields = FIELDS.filter((f) => f.group === "material");
  const calibrationFields = FIELDS.filter((f) => f.group === "calibration");
  const parkFields = FIELDS.filter((f) => f.group === "park");
  const tensionBools = BOOL_FIELDS.filter((f) => f.group === "tension");
  const calibrationBools = BOOL_FIELDS.filter((f) => f.group === "calibration");

  const connected = $derived($wsState === "connected");
  const jobActive = $derived(
    $jobProgress?.state === "running" || $jobProgress?.state === "paused",
  );
  // Editing requires a live link and no streaming job (settings writes share
  // the command channel; mid-cut config changes are unsafe).
  const editable = $derived(connected && !jobActive);
  const config = $derived($maslowConfig);

  let draft = $state<Record<string, number>>({});
  let boolDraft = $state<Record<string, boolean>>({});
  let showAnchors = $state(false);
  let busy = $state(false);
  let message = $state("");
  let error = $state("");
  let fetched = $state(false);

  // Lazily read the config once when we first go live; clear the latch on drop.
  $effect(() => {
    if (connected && !fetched && !config) {
      fetched = true;
      load();
    }
    if (!connected) fetched = false;
  });

  // Mirror the firmware config into the editable draft whenever it (re)loads.
  $effect(() => {
    const c = $maslowConfig;
    if (!c) return;
    const d: Record<string, number> = {};
    for (const f of FIELDS) d[f.key] = c[f.key];
    draft = d;
    const b: Record<string, boolean> = {};
    for (const f of BOOL_FIELDS) b[f.key] = c[f.key];
    boolDraft = b;
  });

  async function load() {
    busy = true;
    error = "";
    message = "";
    try {
      await refreshConfig();
    } catch (e) {
      error = `Read failed: ${e}`;
    } finally {
      busy = false;
    }
  }

  function fieldInvalid(f: NumField): boolean {
    const v = draft[f.key];
    return v == null || Number.isNaN(v) || v < f.min || v > f.max;
  }

  function side(ax: number, ay: number, bx: number, by: number): number {
    return Math.hypot(ax - bx, ay - by);
  }

  // Mirror MaslowKinematics geometry sanity: top above bottom, left of right,
  // and every side within the firmware's 500–5500 mm range.
  const geomValid = $derived.by(() => {
    const d = draft;
    if (d.tl_x == null) return true;
    const topAboveBottom = d.tl_y > d.bl_y && d.tr_y > d.br_y;
    const leftOfRight = d.tl_x < d.tr_x;
    const sides = [
      side(d.tl_x, d.tl_y, d.tr_x, d.tr_y),
      side(d.tr_x, d.tr_y, d.br_x, d.br_y),
      side(d.bl_x, d.bl_y, d.br_x, d.br_y),
      side(d.tl_x, d.tl_y, d.bl_x, d.bl_y),
    ];
    const inRange = sides.every((s) => s >= 500 && s <= 5500);
    return topAboveBottom && leftOfRight && inRange;
  });

  function isDirty(f: NumField): boolean {
    return !!config && draft[f.key] !== config[f.key];
  }
  function isBoolDirty(f: BoolField): boolean {
    return !!config && boolDraft[f.key] !== config[f.key];
  }

  const anchorsDirty = $derived(anchorFields.some((f) => isDirty(f)));
  const dirty = $derived(
    !!config &&
      (FIELDS.some((f) => isDirty(f)) || BOOL_FIELDS.some((f) => isBoolDirty(f))),
  );
  const anyInvalid = $derived(FIELDS.some((f) => fieldInvalid(f)));
  // Block saving anchors that would corrupt geometry; non-anchor edits are
  // never blocked by the anchor geometry check.
  const canSave = $derived(
    editable && dirty && !anyInvalid && !busy && (!anchorsDirty || geomValid),
  );

  function revert() {
    if (!config) return;
    const d: Record<string, number> = {};
    for (const f of FIELDS) d[f.key] = config[f.key];
    draft = d;
    const b: Record<string, boolean> = {};
    for (const f of BOOL_FIELDS) b[f.key] = config[f.key];
    boolDraft = b;
    message = "";
    error = "";
  }

  async function save() {
    if (!config) return;
    busy = true;
    error = "";
    message = "";
    const host = $connection.host;
    try {
      for (const f of FIELDS) {
        if (draft[f.key] !== config[f.key]) {
          await invoke("write_maslow_setting", {
            host,
            path: f.path,
            value: String(draft[f.key]),
          });
        }
      }
      for (const f of BOOL_FIELDS) {
        if (boolDraft[f.key] !== config[f.key]) {
          await invoke("write_maslow_setting", {
            host,
            path: f.path,
            value: boolDraft[f.key] ? "true" : "false",
          });
        }
      }
      await invoke("save_maslow_config", { host });
      message = "Saved to machine flash.";
      await refreshConfig();
    } catch (e) {
      error = `Write failed: ${e}`;
    } finally {
      busy = false;
    }
  }
</script>

<section class="cfg">
  <header>
    <span>Maslow config</span>
    {#if config}
      <span class="geom" class:bad={!config.anchors_valid}>
        {config.anchors_valid ? "geometry valid" : "geometry invalid"}
      </span>
    {/if}
    <button class="ghost read" onclick={load} disabled={!connected || busy}>
      {busy ? "…" : "Read"}
    </button>
  </header>

  {#if !config}
    <div class="hint">
      {connected
        ? busy
          ? "reading configuration…"
          : "press Read to load the machine configuration"
        : "connect to read/edit the configuration"}
    </div>
  {:else}
    {#if !editable}
      <div class="note">
        {jobActive
          ? "Read-only while a job is running."
          : "Read-only — connect to edit."}
      </div>
    {/if}

    {#snippet numField(f: NumField)}
      <label class:dirty={isDirty(f)} class:invalid={fieldInvalid(f)} title={f.help}>
        <span>{f.label} <abbr title={f.help}>ⓘ</abbr></span>
        <input
          type="number"
          step={f.step}
          min={f.min}
          max={f.max}
          bind:value={draft[f.key]}
          disabled={!editable}
        />
      </label>
    {/snippet}

    {#snippet boolField(f: BoolField)}
      <label class="boolrow" class:dirty={isBoolDirty(f)} title={f.help}>
        <input
          type="checkbox"
          bind:checked={boolDraft[f.key]}
          disabled={!editable}
        />
        <span>{f.label} <abbr title={f.help}>ⓘ</abbr></span>
      </label>
    {/snippet}

    <div class="grp">
      <h4>Work area <small>mm</small></h4>
      <div class="grid">
        {#each workFields as f (f.key)}{@render numField(f)}{/each}
      </div>
    </div>

    <div class="grp">
      <h4>Belt tension &amp; extension</h4>
      <div class="grid">
        {#each tensionFields as f (f.key)}{@render numField(f)}{/each}
        {#each tensionBools as f (f.key)}{@render boolField(f)}{/each}
      </div>
    </div>

    <div class="grp">
      <h4>Material <small>mm</small></h4>
      <div class="grid">
        {#each materialFields as f (f.key)}{@render numField(f)}{/each}
      </div>
    </div>

    <div class="grp">
      <h4>Calibration tuning</h4>
      <div class="grid">
        {#each calibrationFields as f (f.key)}{@render numField(f)}{/each}
        {#each calibrationBools as f (f.key)}{@render boolField(f)}{/each}
      </div>
    </div>

    <div class="grp">
      <h4>Park position</h4>
      <div class="grid">
        {#each parkFields as f (f.key)}{@render numField(f)}{/each}
      </div>
    </div>

    <div class="grp">
      <h4 class="adv">
        <label class="toggle">
          <input type="checkbox" bind:checked={showAnchors} />
          <span>Frame anchors <small>advanced</small></span>
        </label>
      </h4>
      {#if showAnchors}
        <div class="warn">
          Anchors are normally set by calibration. Editing them by hand changes
          the machine geometry — only do this if you know the measured frame
          coordinates.
        </div>
        {#if anchorsDirty && !geomValid}
          <div class="geomwarn">
            Geometry check failed (top above bottom, left of right, each side
            500–5500 mm). Anchor changes are blocked until corrected.
          </div>
        {/if}
        <div class="grid anchors">
          {#each anchorFields as f (f.key)}{@render numField(f)}{/each}
        </div>
      {/if}
    </div>

    <div class="bar">
      <button class="go" onclick={save} disabled={!canSave}>
        Save to machine
      </button>
      <button class="ghost" onclick={revert} disabled={!dirty || busy}>
        Revert
      </button>
      {#if dirty}<span class="dot">unsaved changes</span>{/if}
    </div>

    {#if message}<div class="ok">{message}</div>{/if}
    {#if error}<div class="err">{error}</div>{/if}
  {/if}
</section>

<style>
  .cfg {
    background: #161616;
    border: 1px solid #333;
    border-radius: 10px;
    padding: 0.7em 0.9em;
    display: flex;
    flex-direction: column;
    gap: 0.7em;
  }
  header {
    display: flex;
    align-items: center;
    gap: 0.7em;
    font-size: 0.85em;
  }
  header > span:first-child {
    opacity: 0.85;
  }
  .geom {
    font-size: 0.72em;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    color: #3ddc84;
    padding: 0.12em 0.5em;
    border-radius: 6px;
    background: #14301f;
  }
  .geom.bad {
    color: #ff8a8a;
    background: #341414;
  }
  .read {
    margin-left: auto;
  }
  .hint,
  .note {
    font-size: 0.8em;
    opacity: 0.6;
    padding: 0.2em 0;
  }
  .note {
    color: #e0a83d;
    opacity: 0.85;
  }
  .grp {
    display: flex;
    flex-direction: column;
    gap: 0.4em;
  }
  h4 {
    margin: 0.2em 0 0;
    font-size: 0.78em;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    opacity: 0.7;
    font-weight: 600;
  }
  h4 small {
    text-transform: none;
    letter-spacing: 0;
    opacity: 0.6;
    font-weight: 400;
  }
  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 6px;
  }
  .grid.anchors {
    grid-template-columns: 1fr 1fr 1fr;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 2px;
    background: #1f1f1f;
    border: 1px solid transparent;
    border-radius: 7px;
    padding: 0.35em 0.5em;
    font-size: 0.78em;
  }
  label > span {
    opacity: 0.65;
  }
  label small {
    opacity: 0.5;
    font-size: 0.85em;
  }
  label abbr {
    cursor: help;
    text-decoration: none;
    opacity: 0.5;
    font-size: 0.85em;
  }
  label.boolrow {
    flex-direction: row;
    align-items: center;
    gap: 0.5em;
  }
  label.boolrow input {
    width: auto;
  }
  label.boolrow.dirty {
    border-color: #b8860b;
  }
  label.dirty {
    border-color: #b8860b;
  }
  label.invalid {
    border-color: #8b2e2e;
  }
  label.invalid input {
    color: #ff8a8a;
  }
  input[type="number"] {
    width: 100%;
    box-sizing: border-box;
    padding: 0.35em 0.45em;
    border-radius: 6px;
    border: 1px solid #444;
    background: #2b2b2b;
    color: #fff;
    font-variant-numeric: tabular-nums;
    font-size: 1em;
  }
  input:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  label.toggle {
    grid-column: 1 / -1;
    flex-direction: row;
    align-items: center;
    gap: 0.5em;
    background: transparent;
    padding: 0.3em 0;
  }
  label.toggle span {
    opacity: 0.85;
  }
  label.toggle {
    margin: 0;
    padding: 0;
  }
  h4.adv {
    opacity: 1;
  }
  .warn {
    font-size: 0.76em;
    line-height: 1.35;
    color: #e0a83d;
    background: #2a2008;
    border: 1px solid #6b4a1f;
    border-radius: 7px;
    padding: 0.4em 0.6em;
  }
  .geomwarn {
    font-size: 0.76em;
    line-height: 1.35;
    color: #ff8a8a;
    background: #341414;
    border: 1px solid #6b2020;
    border-radius: 7px;
    padding: 0.4em 0.6em;
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 0.5em;
    margin-top: 0.2em;
  }
  .dot {
    font-size: 0.75em;
    color: #e0a83d;
  }
  button {
    padding: 0.45em 0.9em;
    border-radius: 8px;
    border: 1px solid #555;
    background: #2b2b2b;
    color: #fff;
    cursor: pointer;
    font-size: 0.82em;
  }
  button:disabled {
    opacity: 0.35;
    cursor: not-allowed;
  }
  button.go {
    background: #2e7d32;
    border-color: #2e7d32;
  }
  button.ghost {
    background: transparent;
  }
  .ok {
    font-size: 0.8em;
    color: #3ddc84;
  }
  .err {
    font-size: 0.8em;
    color: #ff8a8a;
    word-break: break-word;
  }
</style>
