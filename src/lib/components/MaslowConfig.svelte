<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { wsState } from "$lib/stores/machine";
  import { connection } from "$lib/stores/connection";
  import { jobProgress } from "$lib/stores/job";
  import { fullConfig, refreshConfig, anchors } from "$lib/stores/maslow";
  import { configEntry } from "$lib/stores/config";
  import { DESCRIPTORS, describe, type ResolvedField } from "$lib/stores/configDescriptors";

  // Curated Maslow settings editor. Unlike the raw FluidNC tree, this view is a
  // labelled, bounded, grouped subset. It is driven entirely by the descriptor
  // registry intersected with what the machine actually reports: a curated field
  // only shows when the firmware exposes its key, so version differences (a key
  // that exists on v1.22 but not v1.21) resolve themselves with no per-key code.
  // Values are read from and written to the discovered config by path; there is
  // no parallel typed struct.

  const KIN = "kinematics/MaslowKinematics";
  const GROUP_ORDER = [
    "Work area",
    "Belt tension",
    "Material",
    "Calibration",
    "Park",
    "Frame anchors",
  ];

  const connected = $derived($wsState === "connected");
  const jobActive = $derived(
    $jobProgress?.state === "running" || $jobProgress?.state === "paused",
  );
  // Editing needs a live link and no streaming job (settings writes share the
  // command channel; mid-cut config changes are unsafe).
  const editable = $derived(connected && !jobActive);
  const loaded = $derived($fullConfig != null);

  // Edited values keyed by path; only paths present here are written.
  let draft = $state<Record<string, string>>({});
  let showAnchors = $state(false);
  let busy = $state(false);
  let message = $state("");
  let error = $state("");
  let fetched = $state(false);

  // The curated fields: descriptors that belong to a group AND whose exact path
  // the machine currently reports. Anything the firmware does not expose simply
  // does not appear.
  const fields = $derived.by<ResolvedField[]>(() => {
    const entries = $fullConfig;
    if (!entries) return [];
    const present = new Set(entries.map((e) => e.path));
    return DESCRIPTORS.filter((d) => d.group && present.has(d.match)).map((d) =>
      describe(d.match),
    );
  });

  const grouped = $derived.by(() => {
    const by = new Map<string, ResolvedField[]>();
    for (const f of fields) {
      const g = f.group ?? "Other";
      const list = by.get(g) ?? [];
      list.push(f);
      by.set(g, list);
    }
    return GROUP_ORDER.filter((g) => by.has(g)).map(
      (g) => [g, by.get(g)!] as const,
    );
  });
  const mainGroups = $derived(grouped.filter(([g]) => g !== "Frame anchors"));
  const anchorFields = $derived(
    grouped.find(([g]) => g === "Frame anchors")?.[1] ?? [],
  );

  // Lazily read the config once when we first go live; clear the latch on drop.
  $effect(() => {
    if (connected && !fetched && !loaded) {
      fetched = true;
      load();
    }
    if (!connected) fetched = false;
  });

  function entryVal(path: string): string {
    return configEntry($fullConfig, path)?.value ?? "";
  }
  const cur = (path: string) => draft[path] ?? entryVal(path);
  const isDirty = (path: string) =>
    draft[path] !== undefined && draft[path] !== entryVal(path);
  function set(path: string, v: string) {
    draft = { ...draft, [path]: v };
  }

  function numInvalid(f: ResolvedField): boolean {
    if (f.widget === "bool") return false;
    const v = Number(cur(f.path));
    if (!Number.isFinite(v)) return true;
    if (f.min != null && v < f.min) return true;
    if (f.max != null && v > f.max) return true;
    return false;
  }

  function side(ax: number, ay: number, bx: number, by: number): number {
    return Math.hypot(ax - bx, ay - by);
  }

  // Mirror MaslowKinematics geometry sanity: top above bottom, left of right,
  // each side within the firmware's 500 to 5500 mm range.
  const geomValid = $derived.by(() => {
    const n = (c: string) => Number(cur(`${KIN}/${c}`));
    if (!configEntry($fullConfig, `${KIN}/tlX`)) return true;
    const topAboveBottom = n("tlY") > n("blY") && n("trY") > n("brY");
    const leftOfRight = n("tlX") < n("trX");
    const sides = [
      side(n("tlX"), n("tlY"), n("trX"), n("trY")),
      side(n("trX"), n("trY"), n("brX"), n("brY")),
      side(n("blX"), n("blY"), n("brX"), n("brY")),
      side(n("tlX"), n("tlY"), n("blX"), n("blY")),
    ];
    const inRange = sides.every((s) => s >= 500 && s <= 5500);
    return topAboveBottom && leftOfRight && inRange;
  });

  const anchorsDirty = $derived(anchorFields.some((f) => isDirty(f.path)));
  const dirtyPaths = $derived(fields.filter((f) => isDirty(f.path)));
  const dirty = $derived(dirtyPaths.length > 0);
  const anyInvalid = $derived(fields.some((f) => numInvalid(f)));
  const canSave = $derived(
    editable && dirty && !anyInvalid && !busy && (!anchorsDirty || geomValid),
  );

  async function load() {
    busy = true;
    error = "";
    message = "";
    try {
      await refreshConfig();
      draft = {};
    } catch (e) {
      error = `Read failed: ${e}`;
    } finally {
      busy = false;
    }
  }

  function revert() {
    draft = {};
    message = "";
    error = "";
  }

  async function save() {
    busy = true;
    error = "";
    message = "";
    const host = $connection.host;
    try {
      for (const f of dirtyPaths) {
        await invoke("write_maslow_setting", {
          host,
          path: f.path,
          value: cur(f.path),
        });
      }
      await invoke("save_maslow_config", { host });
      message = "Saved to machine flash.";
      await load();
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
    {#if $anchors}
      <span class="geom" class:bad={!$anchors.valid}>
        {$anchors.valid ? "geometry valid" : "geometry invalid"}
      </span>
    {/if}
    <button class="ghost read" onclick={load} disabled={!connected || busy}>
      {busy ? "…" : "Read"}
    </button>
  </header>

  {#if !loaded}
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

    {#snippet field(f: ResolvedField)}
      {#if f.widget === "bool"}
        <label class="boolrow" class:dirty={isDirty(f.path)} title={f.help}>
          <input
            type="checkbox"
            checked={cur(f.path) === "true"}
            disabled={!editable}
            onchange={(ev) => set(f.path, ev.currentTarget.checked ? "true" : "false")}
          />
          <span>{f.label} {#if f.help}<abbr title={f.help}>ⓘ</abbr>{/if}</span>
        </label>
      {:else}
        <label class:dirty={isDirty(f.path)} class:invalid={numInvalid(f)} title={f.help}>
          <span>{f.label} {#if f.help}<abbr title={f.help}>ⓘ</abbr>{/if}</span>
          <input
            type="number"
            step={f.step}
            min={f.min}
            max={f.max}
            value={cur(f.path)}
            disabled={!editable}
            oninput={(ev) => set(f.path, ev.currentTarget.value)}
          />
        </label>
      {/if}
    {/snippet}

    {#each mainGroups as [group, gfields] (group)}
      <div class="grp">
        <h4>{group}</h4>
        <div class="grid">
          {#each gfields as f (f.path)}{@render field(f)}{/each}
        </div>
      </div>
    {/each}

    {#if anchorFields.length > 0}
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
            the machine geometry; only do this if you know the measured frame
            coordinates.
          </div>
          {#if anchorsDirty && !geomValid}
            <div class="geomwarn">
              Geometry check failed (top above bottom, left of right, each side
              500 to 5500 mm). Anchor changes are blocked until corrected.
            </div>
          {/if}
          <div class="grid anchors">
            {#each anchorFields as f (f.path)}{@render field(f)}{/each}
          </div>
        {/if}
      </div>
    {/if}

    <div class="bar">
      <button class="go" onclick={save} disabled={!canSave}>Save to machine</button>
      <button class="ghost" onclick={revert} disabled={!dirty || busy}>Revert</button>
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
  label.boolrow.dirty,
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
    margin: 0;
    padding: 0;
  }
  label.toggle span {
    opacity: 0.85;
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
