<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { wsState, machineStatus } from "$lib/stores/machine";
  import { actionPolicy } from "$lib/stores/maslow";
  import { layout } from "$lib/stores/viewport";
  import {
    jobProgress,
    loadedJob,
    loadSavedJob,
    loadToolpath,
    clearToolpath,
    type SavedJob,
  } from "$lib/stores/job";
  import Button from "$lib/components/ui/Button.svelte";
  import Modal from "$lib/components/ui/Modal.svelte";
  import ProgressBar from "$lib/components/ui/ProgressBar.svelte";
  import OverrideControl from "$lib/components/ui/OverrideControl.svelte";
  import GcodeList from "$lib/components/controls/GcodeList.svelte";
  import ToolpathView from "$lib/components/ToolpathView.svelte";
  import FileBrowser from "$lib/components/FileBrowser.svelte";

  const GCODE_FILTER = { name: "G-code", extensions: ["nc", "gcode", "gc", "ngc", "tap", "cnc", "txt"] };

  const loaded = $derived($loadedJob);
  let saved = $state<SavedJob | null>(null);
  let busy = $state(false);
  let notice = $state("");
  let showSd = $state(false);
  let sdStarted = $state(false);
  let sdSawRun = $state(false);

  const connected = $derived($wsState === "connected");
  const job = $derived($jobProgress);
  const fluidState = $derived($machineStatus?.state ?? null);
  const machineBusy = $derived(
    fluidState === "Run" ||
      fluidState === "Hold" ||
      fluidState === "Cycle" ||
      fluidState === "Door" ||
      fluidState === "Jog" ||
      fluidState === "Home" ||
      fluidState === "Homing",
  );
  const localActive = $derived(job !== null && job.state !== "done" && job.state !== "error");
  const sdActive = $derived(sdStarted && machineBusy);
  const active = $derived(localActive || sdActive);
  const percent = $derived(job && job.total > 0 ? Math.round((job.acked / job.total) * 100) : 0);
  const canRun = $derived(connected && ($actionPolicy?.run ?? false));
  const alarm = $derived(fluidState === "Alarm");
  const canResumeInterrupted = $derived(connected && !(job?.state === "interrupted" && alarm));
  const canHold = $derived(connected && ($actionPolicy?.hold ?? false));
  const canResumeSd = $derived(connected && ($actionPolicy?.resume ?? false));
  const feedOv = $derived($machineStatus?.ov?.[0] ?? 100);
  // On desktop the list + toolpath sit beside the controls; on tablet/phone they
  // collapse into accordions (closed by default) so the controls aren't buried.
  const stacked = $derived($layout !== "desktop");

  $effect(() => {
    if (!sdStarted) return;
    if (machineBusy) {
      sdSawRun = true;
    } else if (sdSawRun && (fluidState === "Idle" || fluidState === "Alarm")) {
      sdStarted = false;
      sdSawRun = false;
    }
  });

  onMount(async () => {
    saved = await loadSavedJob();
    if (saved?.path) loadToolpath(saved.path);
  });

  const basename = (p: string) => p.split(/[\\/]/).pop() ?? p;

  async function pickFile(): Promise<string | null> {
    const sel = await open({ multiple: false, directory: false, filters: [GCODE_FILTER] });
    return typeof sel === "string" ? sel : null;
  }
  async function chooseLocal() {
    const p = await pickFile();
    if (!p) return;
    loadedJob.set({ source: "local", path: p, name: basename(p) });
    notice = "";
    loadToolpath(p);
  }
  function chooseSd(f: { path: string; name: string }) {
    loadedJob.set({ source: "sd", path: f.path, name: f.name });
    notice = "";
    showSd = false;
  }
  function unload() {
    loadedJob.set(null);
    notice = "";
    clearToolpath();
  }

  async function start() {
    if (!loaded || !canRun) return;
    const where = loaded.source === "sd" ? "from the SD card" : "";
    if (
      !window.confirm(
        `Start cutting ${loaded.name} ${where}? The router will move — make sure the bit, material and work zero are set.`,
      )
    )
      return;
    busy = true;
    try {
      if (loaded.source === "local") {
        const total = await invoke<number>("stream_start", { path: loaded.path, startIndex: 0 });
        notice = `Streaming ${total} lines`;
      } else {
        await invoke("send_line", { line: `$SD/Run=${loaded.path}` });
        sdStarted = true;
        sdSawRun = false;
        notice = `Running ${loaded.name} from SD`;
      }
    } catch (e) {
      notice = `Error: ${e}`;
    } finally {
      busy = false;
    }
  }

  async function resumeSaved() {
    if (!saved || !canRun) return;
    if (
      !window.confirm(
        `Resume ${saved.name} at line ${saved.acked}/${saved.total}? The router will move from the current position.`,
      )
    )
      return;
    busy = true;
    try {
      await invoke<number>("stream_start", { path: saved.path, startIndex: saved.acked });
      notice = `Resumed at line ${saved.acked}`;
      saved = null;
    } catch (e) {
      notice = `Error: ${e}`;
    } finally {
      busy = false;
    }
  }

  const pause = () => invoke("stream_pause");
  const resume = () => invoke("stream_resume");
  const stop = () => invoke("stream_stop");
  const sdHold = () => invoke("send_realtime", { byte: 0x21 });
  const sdResume = () => invoke("send_realtime", { byte: 0x7e });
  function sdStop() {
    if (
      !window.confirm(
        "Abort the SD job? This soft-resets the machine (Ctrl-X) and ends the cut. It will go into Alarm — home or unlock afterward.",
      )
    )
      return;
    invoke("send_realtime", { byte: 0x18 });
  }

  function realtime(byte: number) {
    invoke("send_realtime", { byte });
  }
</script>

<div class="run-tab" class:stacked>
  <div class="col control-col">
    {#if saved && !active && !loaded}
      <div class="resume-banner">
        <span>Interrupted: <strong>{saved.name}</strong> at line {saved.acked}/{saved.total}</span>
        <div class="row">
          <Button size="sm" variant="active" disabled={!canRun || busy} onclick={resumeSaved}>Resume</Button>
          <Button size="sm" variant="ghost" onclick={() => (saved = null)}>Dismiss</Button>
        </div>
      </div>
    {/if}

    {#if !active}
      {#if loaded}
        <div class="loaded">
          <span class="src-badge {loaded.source}">{loaded.source === "sd" ? "SD" : "LOCAL"}</span>
          <span class="filename" title={loaded.path}>{loaded.name}</span>
          <Button size="sm" variant="ghost" disabled={busy} onclick={unload}>Change</Button>
        </div>
        <Button variant="active" size="lg" disabled={!canRun || busy} onclick={start}>▶ Start</Button>
      {:else}
        <Button variant="action" disabled={busy} onclick={chooseLocal}>Open local G-code…</Button>
        <Button variant="ghost" disabled={!connected} onclick={() => (showSd = true)}>Browse SD card…</Button>
      {/if}
    {/if}

    {#if localActive && job}
      <ProgressBar
        value={job.acked}
        max={job.total}
        variant={job.state === "error" ? "warn" : job.state === "running" ? "active" : "warn"}
      />
      <div class="meta">
        <span>{job.acked}/{job.total} lines</span>
        <span>{percent}%</span>
        {#if job.errors > 0}<span class="err">{job.errors} err</span>{/if}
      </div>
      <div class="controls">
        {#if job.state === "running"}
          <Button variant="datum" disabled={!connected} onclick={pause}>⏸ Pause</Button>
        {:else}
          <Button variant="active" disabled={!canResumeInterrupted} onclick={resume}>▶ Resume</Button>
        {/if}
        <Button variant="danger" onclick={stop}>⊗ Cancel</Button>
      </div>
    {:else if sdActive}
      <div class="loaded">
        <span class="src-badge sd">SD</span>
        <span class="filename">{loaded?.name ?? "SD job"}</span>
        <span class="run-state">{fluidState}</span>
      </div>
      <div class="controls">
        {#if fluidState === "Hold"}
          <Button variant="active" disabled={!canResumeSd} onclick={sdResume}>▶ Resume</Button>
        {:else}
          <Button variant="datum" disabled={!canHold} onclick={sdHold}>⏸ Pause</Button>
        {/if}
        <Button variant="danger" onclick={sdStop}>⊗ Cancel</Button>
      </div>
    {/if}

    {#if notice}<p class="notice">{notice}</p>{/if}
    {#if !active && loaded && connected && !canRun}
      <p class="gate-hint">Machine must be <strong>Ready to Cut</strong> (calibrated and tensioned) and idle.</p>
    {/if}
    {#if job?.state === "interrupted" && alarm}
      <p class="gate-hint">Machine is in <strong>Alarm</strong> after reconnecting — home or unlock it, then resume.</p>
    {/if}

    <OverrideControl
      label="Feed override"
      value={feedOv}
      disabled={!connected}
      onUp={() => realtime(0x91)}
      onDown={() => realtime(0x92)}
      onReset={() => realtime(0x90)}
    />
  </div>

  <div class="viewport-col">
    <ToolpathView />
  </div>

  <!-- The textual move list is the nerdy bit: collapsed by default. -->
  <details class="acc gcode">
    <summary>G-code lines</summary>
    <div class="acc-body list-stacked"><GcodeList /></div>
  </details>
</div>

{#if showSd}
  <Modal title="SD Files" onclose={() => (showSd = false)}>
    <FileBrowser onselect={chooseSd} />
  </Modal>
{/if}

<style>
  .run-tab {
    display: grid;
    grid-template-columns: minmax(360px, 1.8fr) minmax(300px, 340px);
    grid-template-rows: minmax(0, 1fr) auto;
    grid-template-areas:
      "viewport control"
      "gcode gcode";
    gap: var(--gap-lg);
    padding: var(--gap-lg);
    height: 100%;
    min-height: 0;
  }
  /* Tablet/phone: single column. Toolpath stays visible; the line list collapses. */
  .run-tab.stacked {
    display: flex;
    flex-direction: column;
    gap: var(--gap);
    padding: var(--gap);
    height: auto;
  }
  .run-tab.stacked .control-col {
    overflow: visible;
  }
  .acc {
    grid-area: gcode;
    background: var(--surface);
    border: 1px solid var(--border-2);
    border-radius: var(--radius-lg);
    padding: var(--gap);
  }
  .acc > summary {
    cursor: pointer;
    font-weight: 600;
    color: var(--text-dim);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    font-size: 0.9em;
    user-select: none;
  }
  .acc[open] > summary {
    margin-bottom: var(--gap);
  }
  .acc-body.list-stacked {
    height: 320px;
  }
  .viewport-col {
    grid-area: viewport;
    min-width: 0;
    min-height: 0;
    overflow-y: auto;
  }
  .control-col {
    grid-area: control;
    display: flex;
    flex-direction: column;
    gap: var(--gap);
    min-width: 0;
    min-height: 0;
    overflow-y: auto;
  }
  .controls {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--gap-sm);
  }
  .resume-banner {
    display: flex;
    flex-direction: column;
    gap: var(--gap-sm);
    background: #2a2410;
    border: 1px solid #5a4b18;
    border-radius: var(--radius);
    padding: 0.6em 0.7em;
    font-size: 0.85em;
  }
  .resume-banner .row {
    display: flex;
    gap: var(--gap-sm);
  }
  .loaded {
    display: flex;
    align-items: center;
    gap: 0.6em;
    background: var(--surface-2);
    border: 1px solid var(--border-2);
    border-radius: var(--radius);
    padding: 0.5em 0.6em;
  }
  .src-badge {
    font-size: 0.68em;
    font-weight: 700;
    letter-spacing: 0.05em;
    padding: 0.25em 0.5em;
    border-radius: 5px;
    white-space: nowrap;
  }
  .src-badge.local {
    background: #20304d;
    color: #9bb4d8;
  }
  .src-badge.sd {
    background: #2a2410;
    color: var(--warn);
  }
  .filename {
    flex: 1;
    font-size: 0.85em;
    color: var(--text-dim);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--mono);
  }
  .run-state {
    font-weight: 600;
    color: #3ddc84;
    font-size: 0.85em;
  }
  .notice {
    margin: 0;
    font-size: 0.82em;
    color: #7fb2ff;
  }
  .gate-hint {
    margin: 0;
    font-size: 0.8em;
    line-height: 1.35;
    color: var(--warn);
  }
  .gate-hint strong {
    color: #ffd166;
  }
  .meta {
    display: flex;
    align-items: center;
    gap: var(--gap);
    font-family: var(--mono);
    font-variant-numeric: tabular-nums;
    font-size: 0.85em;
    color: var(--text-dim);
  }
  .meta .err {
    color: #ff6b6b;
  }

</style>
