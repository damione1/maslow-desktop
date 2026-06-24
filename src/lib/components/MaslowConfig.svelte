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

  type NumKey = Exclude<keyof MaslowConfig, "anchors_valid">;

  interface NumField {
    key: NumKey;
    /** Full FluidNC config path used in `$/<path>` and `$/<path>=<value>`. */
    path: string;
    label: string;
    min: number;
    max: number;
    step: number;
    group: "anchor" | "work" | "tension";
  }

  const KIN = "kinematics/MaslowKinematics";

  // Bounds mirror the firmware (MachineConfig::groupM4Items handler.item ranges
  // and MaslowKinematics validation). NB: the firmware does NOT clamp runtime
  // float writes, so client-side validation is the real guard for those.
  const FIELDS: NumField[] = [
    // Anchors (mm) — advanced. X/Y allow the slightly-negative real frame
    // values (tlX ≈ -27.6); Z is the anchor height.
    { key: "tl_x", path: `${KIN}/tlX`, label: "TL X", min: -500, max: 10000, step: 0.1, group: "anchor" },
    { key: "tl_y", path: `${KIN}/tlY`, label: "TL Y", min: -500, max: 10000, step: 0.1, group: "anchor" },
    { key: "tl_z", path: `${KIN}/tlZ`, label: "TL Z", min: 0, max: 500, step: 0.1, group: "anchor" },
    { key: "tr_x", path: `${KIN}/trX`, label: "TR X", min: -500, max: 10000, step: 0.1, group: "anchor" },
    { key: "tr_y", path: `${KIN}/trY`, label: "TR Y", min: -500, max: 10000, step: 0.1, group: "anchor" },
    { key: "tr_z", path: `${KIN}/trZ`, label: "TR Z", min: 0, max: 500, step: 0.1, group: "anchor" },
    { key: "bl_x", path: `${KIN}/blX`, label: "BL X", min: -500, max: 10000, step: 0.1, group: "anchor" },
    { key: "bl_y", path: `${KIN}/blY`, label: "BL Y", min: -500, max: 10000, step: 0.1, group: "anchor" },
    { key: "bl_z", path: `${KIN}/blZ`, label: "BL Z", min: 0, max: 500, step: 0.1, group: "anchor" },
    { key: "br_x", path: `${KIN}/brX`, label: "BR X", min: -500, max: 10000, step: 0.1, group: "anchor" },
    { key: "br_y", path: `${KIN}/brY`, label: "BR Y", min: -500, max: 10000, step: 0.1, group: "anchor" },
    { key: "br_z", path: `${KIN}/brZ`, label: "BR Z", min: 0, max: 500, step: 0.1, group: "anchor" },
    // Work area (mm).
    { key: "work_area_x", path: "Maslow_Work_Area_X", label: "Width X", min: 1, max: 10000, step: 1, group: "work" },
    { key: "work_area_y", path: "Maslow_Work_Area_Y", label: "Height Y", min: 1, max: 10000, step: 1, group: "work" },
    { key: "work_area_center_offset_x", path: "Maslow_Work_Area_Center_Offset_X", label: "Center offset X", min: -5000, max: 5000, step: 1, group: "work" },
    { key: "work_area_center_offset_y", path: "Maslow_Work_Area_Center_Offset_Y", label: "Center offset Y", min: -5000, max: 5000, step: 1, group: "work" },
    // Belt tension / extension. NB: the Apply_Tension_* keys were removed —
    // they only exist on firmware ≥ v1.22.0 and v1.21 rejects them (error:3).
    { key: "retract_current_threshold", path: "Maslow_Retract_Current_Threshold", label: "Retract current threshold", min: 0, max: 3500, step: 50, group: "tension" },
    { key: "extend_dist", path: "Maslow_Extend_Dist", label: "Extend distance", min: 0, max: 4250, step: 10, group: "tension" },
  ];

  const anchorFields = FIELDS.filter((f) => f.group === "anchor");
  const workFields = FIELDS.filter((f) => f.group === "work");
  const tensionFields = FIELDS.filter((f) => f.group === "tension");

  const connected = $derived($wsState === "connected");
  const jobActive = $derived(
    $jobProgress?.state === "running" || $jobProgress?.state === "paused",
  );
  // Editing requires a live link and no streaming job (settings writes share
  // the command channel; mid-cut config changes are unsafe).
  const editable = $derived(connected && !jobActive);
  const config = $derived($maslowConfig);

  let draft = $state<Record<string, number>>({});
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

  const anchorsDirty = $derived(anchorFields.some((f) => isDirty(f)));
  const dirty = $derived(!!config && FIELDS.some((f) => isDirty(f)));
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

    <div class="grp">
      <h4>Work area <small>mm</small></h4>
      <div class="grid">
        {#each workFields as f (f.key)}
          <label class:dirty={isDirty(f)} class:invalid={fieldInvalid(f)}>
            <span>{f.label}</span>
            <input
              type="number"
              step={f.step}
              min={f.min}
              max={f.max}
              bind:value={draft[f.key]}
              disabled={!editable}
            />
          </label>
        {/each}
      </div>
    </div>

    <div class="grp">
      <h4>Belt tension &amp; extension</h4>
      <div class="grid">
        {#each tensionFields as f (f.key)}
          <label class:dirty={isDirty(f)} class:invalid={fieldInvalid(f)}>
            <span>{f.label} <small>{f.min}–{f.max}</small></span>
            <input
              type="number"
              step={f.step}
              min={f.min}
              max={f.max}
              bind:value={draft[f.key]}
              disabled={!editable}
            />
          </label>
        {/each}
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
          {#each anchorFields as f (f.key)}
            <label class:dirty={isDirty(f)} class:invalid={fieldInvalid(f)}>
              <span>{f.label}</span>
              <input
                type="number"
                step={f.step}
                min={f.min}
                max={f.max}
                bind:value={draft[f.key]}
                disabled={!editable}
              />
            </label>
          {/each}
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
