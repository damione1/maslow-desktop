<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { wsState } from "$lib/stores/machine";
  import { connection } from "$lib/stores/connection";
  import { jobProgress } from "$lib/stores/job";
  import { maslowConfig, refreshConfig } from "$lib/stores/maslow";
  import Modal from "./Modal.svelte";

  // The embedded UI's Setup/Options popups, surfaced next to Manual Control.
  // Each config popup edits the same $/Maslow_* keys as the Config tab and
  // persists with $CO; Set Home issues G10 L20 work-offset commands.

  type CfgKey =
    | "work_area_x"
    | "work_area_y"
    | "work_area_center_offset_x"
    | "work_area_center_offset_y"
    | "park_x"
    | "park_y"
    | "park_z"
    | "scale_x"
    | "scale_y"
    | "work_thickness"
    | "spoilboard_thickness"
    | "apply_tension_belt_retraction_limit"
    | "extend_dist";

  interface Field {
    key: CfgKey;
    path: string;
    label: string;
    min: number;
    max: number;
    step: number;
  }

  type CfgPopup = "work" | "park" | "scale" | "tension";

  const POPUPS: Record<CfgPopup, { title: string; hint?: string; fields: Field[] }> = {
    work: {
      title: "Work Area",
      hint: "Usable cutting area, centred on the offset.",
      fields: [
        { key: "work_area_x", path: "Maslow_Work_Area_X", label: "Width X (mm)", min: 1, max: 10000, step: 1 },
        { key: "work_area_y", path: "Maslow_Work_Area_Y", label: "Height Y (mm)", min: 1, max: 10000, step: 1 },
        { key: "work_area_center_offset_x", path: "Maslow_Work_Area_Center_Offset_X", label: "Center offset X (mm)", min: -5000, max: 5000, step: 1 },
        { key: "work_area_center_offset_y", path: "Maslow_Work_Area_Center_Offset_Y", label: "Center offset Y (mm)", min: -5000, max: 5000, step: 1 },
      ],
    },
    park: {
      title: "Park Position",
      hint: "Where the machine moves when you press Park (machine coords).",
      fields: [
        { key: "park_x", path: "Maslow_Park_X", label: "Park X (mm)", min: -10000, max: 10000, step: 1 },
        { key: "park_y", path: "Maslow_Park_Y", label: "Park Y (mm)", min: -10000, max: 10000, step: 1 },
        { key: "park_z", path: "Maslow_Park_Z", label: "Park Z lift (mm)", min: -100, max: 100, step: 0.1 },
      ],
    },
    scale: {
      title: "Scale & Thickness",
      fields: [
        { key: "scale_x", path: "Maslow_Scale_X", label: "Scale X", min: 0.8, max: 1.2, step: 0.001 },
        { key: "scale_y", path: "Maslow_Scale_Y", label: "Scale Y", min: 0.8, max: 1.2, step: 0.001 },
        { key: "work_thickness", path: "Maslow_workThickness", label: "Work thickness (mm)", min: 0, max: 50, step: 0.1 },
        { key: "spoilboard_thickness", path: "Maslow_spoilboardThickness", label: "Spoilboard thickness (mm)", min: 0, max: 50, step: 0.1 },
      ],
    },
    tension: {
      title: "Apply-Tension Limit",
      hint: "Caps belt retraction while applying tension (firmware ≥ v1.22).",
      fields: [
        { key: "apply_tension_belt_retraction_limit", path: "Maslow_Apply_Tension_Belt_Retraction_Limit", label: "Belt retraction limit (mm)", min: 0, max: 4250, step: 10 },
        { key: "extend_dist", path: "Maslow_Extend_Dist", label: "Extend distance (mm)", min: 0, max: 4250, step: 10 },
      ],
    },
  };

  type PopupKey = CfgPopup | "setxy" | "setz";

  let menu = $state(false);
  let open = $state<PopupKey | null>(null);
  let draft = $state<Record<string, number>>({});
  let homeX = $state(0);
  let homeY = $state(0);
  let homeZ = $state(0);
  let busy = $state(false);
  let err = $state("");

  const connected = $derived($wsState === "connected");
  const jobActive = $derived(
    $jobProgress?.state === "running" || $jobProgress?.state === "paused",
  );
  const editable = $derived(connected && !jobActive);
  const config = $derived($maslowConfig);
  const host = $derived($connection.host);

  const cfgKey = $derived<CfgPopup | null>(
    open === "work" || open === "park" || open === "scale" || open === "tension"
      ? open
      : null,
  );

  function invalid(f: Field): boolean {
    const v = draft[f.key];
    return v == null || Number.isNaN(v) || v < f.min || v > f.max;
  }

  function initDraft(key: CfgPopup) {
    const c = $maslowConfig;
    if (!c) return;
    const d: Record<string, number> = {};
    for (const f of POPUPS[key].fields) d[f.key] = c[f.key];
    draft = d;
  }

  async function openConfig(key: CfgPopup) {
    menu = false;
    err = "";
    open = key;
    if (!config) {
      busy = true;
      try {
        await refreshConfig();
      } catch (e) {
        err = `Read failed: ${e}`;
      } finally {
        busy = false;
      }
    }
    initDraft(key);
  }

  const dirty = $derived.by(() => {
    if (!cfgKey || !config) return false;
    return POPUPS[cfgKey].fields.some((f) => draft[f.key] !== config[f.key]);
  });
  const anyInvalid = $derived.by(() => {
    if (!cfgKey) return false;
    return POPUPS[cfgKey].fields.some((f) => invalid(f));
  });
  const canSave = $derived(editable && !busy && dirty && !anyInvalid);

  async function save() {
    if (!cfgKey) return;
    const c = $maslowConfig;
    if (!c) return;
    busy = true;
    err = "";
    try {
      for (const f of POPUPS[cfgKey].fields) {
        if (draft[f.key] !== c[f.key]) {
          await invoke("write_maslow_setting", { host, path: f.path, value: String(draft[f.key]) });
        }
      }
      await invoke("save_maslow_config", { host });
      await refreshConfig();
      open = null;
    } catch (e) {
      err = `Write failed: ${e}`;
    } finally {
      busy = false;
    }
  }

  async function line(cmd: string) {
    busy = true;
    err = "";
    try {
      await invoke("send_line", { line: cmd });
      open = null;
    } catch (e) {
      err = `Failed: ${e}`;
    } finally {
      busy = false;
    }
  }

  function openSetXY() {
    menu = false;
    err = "";
    homeX = 0;
    homeY = 0;
    open = "setxy";
  }
  function openSetZ() {
    menu = false;
    err = "";
    homeZ = 0;
    open = "setz";
  }
</script>

<section class="opts">
  <div class="bar">
    <div class="menu-wrap">
      <button class="ghost" onclick={() => (menu = !menu)} disabled={!connected}>
        Options ▾
      </button>
      {#if menu}
        <div class="menu">
          <button onclick={() => openConfig("work")}>Work Area…</button>
          <button onclick={() => openConfig("park")}>Park…</button>
          <button onclick={() => openConfig("scale")}>Scale &amp; Thickness…</button>
          <button onclick={() => openConfig("tension")}>Apply-Tension Limit…</button>
        </div>
      {/if}
    </div>
    <button class="ghost" onclick={openSetXY} disabled={!editable}>Set XY Home…</button>
    <button class="ghost" onclick={openSetZ} disabled={!editable}>Set Z Home…</button>
  </div>

  {#if cfgKey}
    <Modal title={POPUPS[cfgKey].title} onclose={() => (open = null)}>
      {#if POPUPS[cfgKey].hint}
        <p class="hint">{POPUPS[cfgKey].hint}</p>
      {/if}
      {#if !config}
        <p class="hint">{busy ? "Reading configuration…" : "Configuration unavailable."}</p>
      {:else}
        <div class="fields">
          {#each POPUPS[cfgKey].fields as f (f.key)}
            <label class:invalid={invalid(f)}>
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
      {#if err}<p class="err">{err}</p>{/if}
      {#if !editable}<p class="note">Read-only — connect with no job running to edit.</p>{/if}
      <div class="actions">
        <button class="ghost" onclick={() => (open = null)}>Cancel</button>
        <button class="go" onclick={save} disabled={!canSave}>
          {busy ? "Saving…" : "Save"}
        </button>
      </div>
    </Modal>
  {:else if open === "setxy"}
    <Modal title="Set XY Home" onclose={() => (open = null)}>
      <p class="hint">
        Sets the current position's work coordinates. Leave at 0 to zero XY here.
      </p>
      <div class="fields">
        <label><span>X (mm)</span><input type="number" step="0.1" bind:value={homeX} /></label>
        <label><span>Y (mm)</span><input type="number" step="0.1" bind:value={homeY} /></label>
      </div>
      {#if err}<p class="err">{err}</p>{/if}
      <div class="actions">
        <button class="ghost" onclick={() => (open = null)}>Cancel</button>
        <button class="go" onclick={() => line(`G10 L20 P0 X${homeX} Y${homeY}`)} disabled={!editable || busy}>
          Set Home
        </button>
      </div>
    </Modal>
  {:else if open === "setz"}
    <Modal title="Set Z Home" onclose={() => (open = null)}>
      <p class="hint">
        Sets the current Z work coordinate. Leave at 0 to zero Z here (after
        touching off on the stock).
      </p>
      <div class="fields">
        <label><span>Z (mm)</span><input type="number" step="0.1" bind:value={homeZ} /></label>
      </div>
      {#if err}<p class="err">{err}</p>{/if}
      <div class="actions">
        <button class="ghost" onclick={() => line("G90 G0 Z0")} disabled={!editable || busy}>
          Go to Z Home
        </button>
        <button class="go" onclick={() => line(`G10 L20 P0 Z${homeZ}`)} disabled={!editable || busy}>
          Set Z Home
        </button>
      </div>
    </Modal>
  {/if}
</section>

<style>
  .opts {
    background: #161616;
    border: 1px solid #333;
    border-radius: 10px;
    padding: 0.55em 0.7em;
  }
  .bar {
    display: flex;
    gap: 0.5em;
    flex-wrap: wrap;
  }
  .menu-wrap {
    position: relative;
  }
  .menu {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    z-index: 20;
    display: flex;
    flex-direction: column;
    min-width: 200px;
    background: #1c1c1c;
    border: 1px solid #3a3a3a;
    border-radius: 8px;
    overflow: hidden;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
  }
  .menu button {
    text-align: left;
    background: transparent;
    border: none;
    border-radius: 0;
    color: #e6e6e6;
    padding: 0.55em 0.8em;
  }
  .menu button:hover {
    background: #2a2a2a;
  }
  button {
    padding: 0.4em 0.7em;
    border-radius: 8px;
    border: 1px solid #555;
    background: #2b2b2b;
    color: #fff;
    cursor: pointer;
    font-size: 0.82em;
  }
  button:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
  .ghost {
    background: transparent;
    border-color: #555;
  }
  .go {
    background: #2e7d32;
    border-color: #2e7d32;
  }
  .fields {
    display: grid;
    grid-template-columns: 1fr;
    gap: 0.5em;
    margin: 0.3em 0 0.6em;
  }
  .fields label {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.8em;
    font-size: 0.85em;
  }
  .fields label.invalid input {
    border-color: #c62828;
  }
  .fields input {
    width: 130px;
    padding: 0.35em 0.5em;
    border-radius: 7px;
    border: 1px solid #444;
    background: #2b2b2b;
    color: #fff;
    font-variant-numeric: tabular-nums;
  }
  .hint {
    margin: 0 0 0.5em;
    font-size: 0.8em;
    opacity: 0.7;
    line-height: 1.35;
  }
  .note {
    font-size: 0.78em;
    color: #e0a83d;
    margin: 0.2em 0;
  }
  .err {
    font-size: 0.8em;
    color: #ff6b6b;
    margin: 0.2em 0;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 0.5em;
    margin-top: 0.4em;
  }
  .actions button {
    padding: 0.45em 0.9em;
  }
</style>
