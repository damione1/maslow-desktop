<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { wsState, machineStatus } from "$lib/stores/machine";
  import { jobProgress } from "$lib/stores/job";
  import { connection } from "$lib/stores/connection";
  import { actionPolicy, maslowConfig, refreshConfig } from "$lib/stores/maslow";
  import Modal from "./Modal.svelte";

  // `touch` enlarges the jog pad + controls for shop-floor finger use on
  // phone/tablet; the compact desktop rail leaves it off.
  let { touch = false }: { touch?: boolean } = $props();

  const STEPS = [0.1, 1, 10, 50];
  let step = $state(10);
  let feed = $state(1000);

  const connected = $derived($wsState === "connected");
  const jobActive = $derived(
    $jobProgress !== null &&
      $jobProgress.state !== "done" &&
      $jobProgress.state !== "error",
  );
  // Gating from the unified action policy (FluidNC state + job).
  const ap = $derived($actionPolicy);
  const allow = (key: keyof NonNullable<typeof ap>) =>
    connected && (ap?.[key] ?? false);
  const canJog = $derived(allow("jog"));
  const canHome = $derived(allow("home"));
  const canUnlock = $derived(allow("unlock"));
  const canZero = $derived(allow("zero"));

  function line(cmd: string) {
    invoke("send_line", { line: cmd });
  }

  function realtime(byte: number) {
    invoke("send_realtime", { byte });
  }

  function jog(axis: "X" | "Y" | "Z", dir: 1 | -1) {
    const dist = (dir * step).toString();
    line(`$J=G91 G21 ${axis}${dist} F${feed}`);
  }

  // 0x18 = Ctrl-X soft reset, 0x21 = '!', 0x7e = '~', 0x85 = jog cancel.
  const hold = () => realtime(0x21);
  const resume = () => realtime(0x7e);
  const reset = () => realtime(0x18);
  const jogCancel = () => realtime(0x85);

  // Feed-rate override (realtime, applies live — useful mid-cut). 0x90 = reset to
  // 100%, 0x91 = +10%, 0x92 = -10%. The live percent comes from the status
  // report's `Ov:` field (feed is the first value).
  const feedOverride = $derived($machineStatus?.ov?.[0] ?? 100);
  const feedReset = () => realtime(0x90);
  const feedUp = () => realtime(0x91);
  const feedDown = () => realtime(0x92);

  const home = () => line("$H");
  const unlock = () => line("$X");

  // --- Options + home popups (formerly ControlOptions) --------------------
  // The Options dropdown edits the same $/Maslow_* keys as the Config tab and
  // persists with $CO; Define-home issues G10 L20 work-offset commands.

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

  async function runCmd(cmd: string) {
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

<section class="jog" class:touch>
  <header>
    <span>Manual Control</span>
    {#if jobActive}<span class="hint">locked during job</span>{/if}
  </header>

  <div class="grid">
    <div class="xy">
      <button class="up" onclick={() => jog("Y", 1)} disabled={!canJog}>Y+</button>
      <button class="left" onclick={() => jog("X", -1)} disabled={!canJog}>X−</button>
      <button class="home" onclick={home} disabled={!canHome} title="Home $H">⌂</button>
      <button class="right" onclick={() => jog("X", 1)} disabled={!canJog}>X+</button>
      <button class="down" onclick={() => jog("Y", -1)} disabled={!canJog}>Y−</button>
    </div>

    <div class="z">
      <button onclick={() => jog("Z", 1)} disabled={!canJog}>Z+</button>
      <button onclick={() => jog("Z", -1)} disabled={!canJog}>Z−</button>
    </div>
  </div>

  <div class="row steps">
    <div class="group">
      <span class="lbl">Step</span>
      <div class="chips">
        {#each STEPS as s}
          <button class="chip" class:on={step === s} onclick={() => (step = s)}>
            {s}
          </button>
        {/each}
      </div>
    </div>
    <label class="group">
      <span class="lbl">Feed</span>
      <input class="feed" type="number" min="1" bind:value={feed} />
    </label>
  </div>

  <div class="row">
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
    <button class="ghost" onclick={unlock} disabled={!canUnlock}>Unlock $X</button>
    <button class="ghost" onclick={openSetXY} disabled={!canZero}>Define home XY</button>
    <button class="ghost" onclick={openSetZ} disabled={!canZero}>Define home Z</button>
    <button class="ghost" onclick={jogCancel} disabled={!connected}>Jog Cancel</button>
  </div>

  <div class="row realtime">
    <button class="hold" onclick={hold} disabled={!connected}>Hold !</button>
    <button class="resume" onclick={resume} disabled={!connected}>Resume ~</button>
    <button
      class="danger"
      onclick={reset}
      disabled={!connected}
      title="Soft reset (Ctrl-X / 0x18) — halts all motion. Same action as the top-bar E-STOP."
      >Reset ⌃X</button
    >
  </div>

  <!-- Feed-rate override: realtime, so it applies even mid-cut to slow or speed
       a running job without stopping it. -->
  <div class="row feedov">
    <span class="lbl" title="Feed-rate override (live)">Feed {feedOverride}%</span>
    <button onclick={feedDown} disabled={!connected} title="Feed −10% (0x92)">−</button>
    <button onclick={feedReset} disabled={!connected} title="Reset feed to 100% (0x90)">100%</button>
    <button onclick={feedUp} disabled={!connected} title="Feed +10% (0x91)">+</button>
  </div>
</section>

{#if cfgKey}
  <Modal title={POPUPS[cfgKey].title} onclose={() => (open = null)}>
    {#if POPUPS[cfgKey].hint}
      <p class="phint">{POPUPS[cfgKey].hint}</p>
    {/if}
    {#if !config}
      <p class="phint">{busy ? "Reading configuration…" : "Configuration unavailable."}</p>
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
    {#if err}<p class="perr">{err}</p>{/if}
    {#if !editable}<p class="pnote">Read-only — connect with no job running to edit.</p>{/if}
    <div class="actions">
      <button class="ghost" onclick={() => (open = null)}>Cancel</button>
      <button class="go" onclick={save} disabled={!canSave}>
        {busy ? "Saving…" : "Save"}
      </button>
    </div>
  </Modal>
{:else if open === "setxy"}
  <Modal title="Define home XY" onclose={() => (open = null)}>
    <p class="phint">
      Sets the current position's work coordinates. Leave at 0 to zero XY here.
    </p>
    <div class="fields">
      <label><span>X (mm)</span><input type="number" step="0.1" bind:value={homeX} /></label>
      <label><span>Y (mm)</span><input type="number" step="0.1" bind:value={homeY} /></label>
    </div>
    {#if err}<p class="perr">{err}</p>{/if}
    <div class="actions">
      <button class="ghost" onclick={() => runCmd("G90 G0 X0 Y0")} disabled={!editable || busy}>
        Go to XY Home
      </button>
      <button class="go" onclick={() => runCmd(`G10 L20 P0 X${homeX} Y${homeY}`)} disabled={!editable || busy}>
        Set XY Home
      </button>
    </div>
  </Modal>
{:else if open === "setz"}
  <Modal title="Define home Z" onclose={() => (open = null)}>
    <p class="phint">
      Sets the current Z work coordinate. Leave at 0 to zero Z here (after
      touching off on the stock).
    </p>
    <div class="fields">
      <label><span>Z (mm)</span><input type="number" step="0.1" bind:value={homeZ} /></label>
    </div>
    {#if err}<p class="perr">{err}</p>{/if}
    <div class="actions">
      <button class="ghost" onclick={() => runCmd("G90 G0 Z0")} disabled={!editable || busy}>
        Go to Z Home
      </button>
      <button class="go" onclick={() => runCmd(`G10 L20 P0 Z${homeZ}`)} disabled={!editable || busy}>
        Set Z Home
      </button>
    </div>
  </Modal>
{/if}

<style>
  .jog {
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
    justify-content: space-between;
    align-items: baseline;
    font-size: 0.85em;
    opacity: 0.85;
  }
  .hint {
    font-size: 0.78em;
    color: #e0a83d;
  }
  .grid {
    display: flex;
    gap: 1.2em;
    align-items: center;
  }
  .xy {
    display: grid;
    grid-template-columns: repeat(3, 48px);
    grid-template-rows: repeat(3, 40px);
    gap: 6px;
  }
  .xy .up { grid-column: 2; grid-row: 1; }
  .xy .left { grid-column: 1; grid-row: 2; }
  .xy .home { grid-column: 2; grid-row: 2; }
  .xy .right { grid-column: 3; grid-row: 2; }
  .xy .down { grid-column: 2; grid-row: 3; }
  .z {
    display: grid;
    grid-template-rows: 40px 40px;
    gap: 6px;
    width: 48px;
  }
  button {
    border-radius: 8px;
    border: 1px solid #396cd8;
    background: #2b3a5c;
    color: #fff;
    cursor: pointer;
    font-size: 0.9em;
  }
  button:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
  .home {
    background: #333;
    border-color: #555;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 0.5em;
    flex-wrap: wrap;
  }
  .row button {
    padding: 0.4em 0.9em;
  }
  .steps {
    gap: 1em;
    row-gap: 0.6em;
  }
  .group {
    display: flex;
    align-items: center;
    gap: 0.5em;
  }
  .chips {
    display: flex;
    align-items: center;
    gap: 0.5em;
  }
  .lbl {
    font-size: 0.78em;
    opacity: 0.6;
  }
  .chip {
    background: #222;
    border-color: #444;
    padding: 0.3em 0.7em;
    font-variant-numeric: tabular-nums;
  }
  .chip.on {
    background: #396cd8;
    border-color: #396cd8;
  }
  .feed {
    width: 80px;
    padding: 0.35em 0.5em;
    border-radius: 7px;
    border: 1px solid #444;
    background: #2b2b2b;
    color: #fff;
  }
  .ghost {
    background: transparent;
    border-color: #555;
  }
  .feedov .lbl {
    flex: 1;
    font-variant-numeric: tabular-nums;
  }
  .feedov button {
    padding: 0.3em 0.6em;
    min-width: 2.4em;
    background: #2b2b2b;
    border: 1px solid #444;
  }
  .realtime .hold {
    background: #b8860b;
    border-color: #b8860b;
  }
  .realtime .resume {
    background: #2e7d32;
    border-color: #2e7d32;
  }
  .danger {
    background: #8b2e2e;
    border-color: #8b2e2e;
  }

  /* Options dropdown */
  .menu-wrap {
    position: relative;
  }
  .menu {
    position: absolute;
    bottom: calc(100% + 4px);
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
    font-size: 0.85em;
  }
  .menu button:hover {
    background: #2a2a2a;
  }

  /* Home / config popup contents */
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
  .phint {
    margin: 0 0 0.5em;
    font-size: 0.8em;
    opacity: 0.7;
    line-height: 1.35;
  }
  .pnote {
    font-size: 0.78em;
    color: #e0a83d;
    margin: 0.2em 0;
  }
  .perr {
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
    border-color: #555;
    background: #2b2b2b;
  }
  .actions button.ghost {
    background: transparent;
  }
  .actions button.go {
    background: #2e7d32;
    border-color: #2e7d32;
  }

  /* Touch mode (phone/tablet): jog pad sized for fingers — ~15mm targets per
     industrial HMI guidance — and the pad centred so both thumbs reach it. */
  .jog.touch {
    gap: 1em;
  }
  .jog.touch header {
    font-size: 1em;
  }
  .jog.touch .grid {
    justify-content: center;
    gap: 1.6em;
  }
  .jog.touch .xy {
    grid-template-columns: repeat(3, 76px);
    grid-template-rows: repeat(3, 68px);
    gap: 10px;
  }
  .jog.touch .z {
    grid-template-rows: 68px 68px;
    width: 76px;
    gap: 10px;
  }
  .jog.touch button {
    font-size: 1.1em;
  }
  .jog.touch .row button {
    min-height: 48px;
    padding: 0.5em 1em;
    flex: 1;
  }
  .jog.touch .menu-wrap {
    flex: 1;
  }
  .jog.touch .menu-wrap > button {
    width: 100%;
  }
  .jog.touch .menu button {
    min-height: 44px;
  }
  .jog.touch .chip {
    min-height: 44px;
    min-width: 52px;
    padding: 0.5em 0.9em;
    font-size: 1em;
    flex: 1;
  }
  .jog.touch .chips {
    flex: 1;
  }
  .jog.touch .feed {
    width: 100px;
    min-height: 44px;
  }
  .jog.touch .steps {
    flex-wrap: wrap;
  }
</style>
