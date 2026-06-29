<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { wsState, machineStatus } from "$lib/stores/machine";
  import { actionPolicy } from "$lib/stores/maslow";
  import {
    jobProgress,
    loadSavedJob,
    loadToolpath,
    clearToolpath,
    type SavedJob,
  } from "$lib/stores/job";
  import Modal from "./Modal.svelte";
  import FileBrowser from "./FileBrowser.svelte";

  const GCODE_FILTER = {
    name: "G-code",
    extensions: ["nc", "gcode", "gc", "ngc", "tap", "cnc", "txt"],
  };

  // A job has exactly one source. The operator loads either a local file (the
  // app streams it) or an SD-card file (the firmware runs it), never both at
  // once, and a single Start launches whichever is loaded.
  type Loaded = { source: "local" | "sd"; path: string; name: string };
  let loaded = $state<Loaded | null>(null);

  let saved = $state<SavedJob | null>(null);
  let busy = $state(false);
  let notice = $state<string>("");
  let showSd = $state(false);

  // An SD run is firmware-owned: it emits no stream-progress, so we track that
  // we launched one and watch the machine state to know when it ends.
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

  // A locally streamed job is active while the streamer reports progress; an SD
  // job is active while our launch flag stands and the machine is still moving.
  const localActive = $derived(
    job !== null && job.state !== "done" && job.state !== "error",
  );
  const sdActive = $derived(sdStarted && machineBusy);
  const active = $derived(localActive || sdActive);

  const percent = $derived(
    job && job.total > 0 ? Math.round((job.acked / job.total) * 100) : 0,
  );
  // Streaming/SD-run is allowed only when the machine is Idle AND in
  // READY_TO_CUT — the single state where the firmware powers the XY belt PID.
  // Reconciled in Rust and the same gate for both sources.
  const canRun = $derived(connected && ($actionPolicy?.run ?? false));
  // After a reconnect the firmware may come back in Alarm (it rebooted); never
  // resume an interrupted stream into Alarm — the operator must home/unlock first.
  const alarm = $derived(fluidState === "Alarm");
  const canResumeInterrupted = $derived(
    connected && !(job?.state === "interrupted" && alarm),
  );

  // SD pause/resume reuse the machine realtime gates (reconciled in Rust), the
  // same allow-list the rail's Hold/Resume use.
  const canHold = $derived(connected && ($actionPolicy?.hold ?? false));
  const canResumeSd = $derived(connected && ($actionPolicy?.resume ?? false));

  // Clear the SD launch flag once a run we saw start has returned to rest.
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
    // Preview the resumable job's toolpath too (its file is still local).
    if (saved?.path) loadToolpath(saved.path);
  });

  function basename(p: string): string {
    return p.split(/[\\/]/).pop() ?? p;
  }

  async function pickFile(): Promise<string | null> {
    const sel = await open({
      multiple: false,
      directory: false,
      filters: [GCODE_FILTER],
    });
    return typeof sel === "string" ? sel : null;
  }

  async function chooseLocal() {
    const p = await pickFile();
    if (!p) return;
    loaded = { source: "local", path: p, name: basename(p) };
    notice = "";
    loadToolpath(p);
  }

  // From the SD browser: the toolpath is already loaded there; just record it as
  // the active job and close the modal.
  function chooseSd(f: { path: string; name: string }) {
    loaded = { source: "sd", path: f.path, name: f.name };
    notice = "";
    showSd = false;
  }

  function unload() {
    loaded = null;
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
        const total = await invoke<number>("stream_start", {
          path: loaded.path,
          startIndex: 0,
        });
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
      await invoke<number>("stream_start", {
        path: saved.path,
        startIndex: saved.acked,
      });
      notice = `Resumed at line ${saved.acked}`;
      saved = null;
    } catch (e) {
      notice = `Error: ${e}`;
    } finally {
      busy = false;
    }
  }

  // Local stream controls (app-owned char-counting streamer).
  const pause = () => invoke("stream_pause");
  const resume = () => invoke("stream_resume");
  const stop = () => invoke("stream_stop");

  // SD-run controls map to machine realtime bytes: Hold (!), Resume (~), and a
  // soft reset (Ctrl-X / 0x18) to abort — there is no clean firmware-side stop.
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
</script>

<section class="job">
  <header>
    <span>G-code Job</span>
    {#if notice}<span class="notice">{notice}</span>{/if}
  </header>

  {#if saved && !active && !loaded}
    <div class="resume-banner">
      <span>
        Interrupted job: <strong>{saved.name}</strong>
        at line {saved.acked}/{saved.total}
      </span>
      <div class="row">
        <button onclick={resumeSaved} disabled={!canRun || busy}>Resume</button>
        <button class="ghost" onclick={() => (saved = null)}>Dismiss</button>
      </div>
    </div>
  {/if}

  {#if !active}
    <!-- Source selection: one job at a time, local stream OR SD card. -->
    {#if loaded}
      <div class="loaded">
        <span class="src-badge {loaded.source}">
          {loaded.source === "sd" ? "SD CARD" : "LOCAL"}
        </span>
        <span class="filename" title={loaded.path}>{loaded.name}</span>
        <button class="ghost sm" onclick={unload} disabled={busy}>Change</button>
      </div>
      <div class="controls">
        <button onclick={start} disabled={!canRun || busy}>Start</button>
      </div>
    {:else}
      <div class="source-pick">
        <button class="ghost" onclick={chooseLocal} disabled={busy}>
          Open local G-code…
        </button>
        <button
          class="ghost"
          onclick={() => (showSd = true)}
          disabled={!connected}
        >
          Browse SD card…
        </button>
      </div>
    {/if}
  {/if}

  {#if localActive && job}
    <div class="progress">
      <div class="bar">
        <div
          class="fill"
          class:err={job.state === "error"}
          class:paused={job.state === "paused" || job.state === "interrupted"}
          style="width:{percent}%"
        ></div>
      </div>
      <div class="meta">
        <span class="state state-{job.state}">{job.state}</span>
        <span>{job.acked}/{job.total} ({percent}%)</span>
        {#if job.errors > 0}<span class="errcount">{job.errors} err</span>{/if}
        <span class="name">{job.name}</span>
      </div>
    </div>
    <div class="controls">
      {#if job.state === "running"}
        <button onclick={pause} disabled={!connected}>Pause</button>
      {:else}
        <button onclick={resume} disabled={!canResumeInterrupted}>Resume</button>
      {/if}
      <button class="danger" onclick={stop}>Stop</button>
    </div>
  {:else if sdActive}
    <!-- Firmware-owned SD run: no line progress, so show state + machine controls. -->
    <div class="sd-running">
      <span class="src-badge sd">SD CARD</span>
      <span class="filename" title={loaded?.path}>{loaded?.name ?? "SD job"}</span>
      <span class="state state-running">{fluidState}</span>
    </div>
    <div class="controls">
      {#if fluidState === "Hold"}
        <button onclick={sdResume} disabled={!canResumeSd}>Resume</button>
      {:else}
        <button onclick={sdHold} disabled={!canHold}>Pause</button>
      {/if}
      <button class="danger" onclick={sdStop}>Stop</button>
    </div>
  {/if}

  {#if !active && loaded && connected && !canRun}
    <p class="gate-hint">
      Machine must be <strong>Ready to Cut</strong> (calibrated and tensioned) and
      idle before a job can start.
    </p>
  {/if}
  {#if job?.state === "interrupted" && alarm}
    <p class="gate-hint">
      Machine is in <strong>Alarm</strong> after reconnecting — home or unlock it,
      then resume.
    </p>
  {/if}
</section>

{#if showSd}
  <Modal title="SD Files" onclose={() => (showSd = false)}>
    <FileBrowser onselect={chooseSd} />
  </Modal>
{/if}

<style>
  .job {
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
  .notice {
    font-size: 0.8em;
    color: #7fb2ff;
  }
  .resume-banner {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 0.6em;
    background: #2a2410;
    border: 1px solid #5a4b18;
    border-radius: 8px;
    padding: 0.5em 0.7em;
    font-size: 0.85em;
  }
  .source-pick {
    display: flex;
    gap: 0.6em;
  }
  .source-pick button {
    flex: 1;
  }
  .loaded,
  .sd-running {
    display: flex;
    align-items: center;
    gap: 0.6em;
    background: #1c1c1c;
    border: 1px solid #333;
    border-radius: 8px;
    padding: 0.45em 0.6em;
  }
  .src-badge {
    font-size: 0.68em;
    font-weight: 700;
    letter-spacing: 0.05em;
    padding: 0.2em 0.45em;
    border-radius: 5px;
    white-space: nowrap;
  }
  .src-badge.local {
    background: #20304d;
    color: #9bb4d8;
  }
  .src-badge.sd {
    background: #2a2410;
    color: #e0a83d;
  }
  .filename {
    flex: 1;
    font-size: 0.82em;
    opacity: 0.7;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: "SF Mono", Menlo, Consolas, monospace;
  }
  button.sm {
    padding: 0.25em 0.7em;
    font-size: 0.8em;
  }
  .progress {
    display: flex;
    flex-direction: column;
    gap: 0.35em;
  }
  .bar {
    height: 10px;
    background: #2b2b2b;
    border-radius: 5px;
    overflow: hidden;
  }
  .fill {
    height: 100%;
    background: #3ddc84;
    transition: width 0.2s ease;
  }
  .fill.paused {
    background: #e0a83d;
  }
  .fill.err {
    background: #ff6b6b;
  }
  .meta {
    display: flex;
    align-items: center;
    gap: 0.7em;
    font-size: 0.78em;
    opacity: 0.85;
  }
  .state {
    text-transform: uppercase;
    font-weight: 600;
    letter-spacing: 0.03em;
  }
  .state-running {
    color: #3ddc84;
  }
  .state-paused,
  .state-interrupted {
    color: #e0a83d;
  }
  .state-done {
    color: #7fb2ff;
  }
  .state-error {
    color: #ff6b6b;
  }
  .errcount {
    color: #ff6b6b;
  }
  .name {
    margin-left: auto;
    font-family: "SF Mono", Menlo, Consolas, monospace;
    opacity: 0.6;
  }
  .controls {
    display: flex;
    gap: 0.5em;
  }
  .gate-hint {
    margin: 0;
    font-size: 0.78em;
    line-height: 1.35;
    color: #e0a83d;
  }
  .gate-hint strong {
    color: #ffd166;
  }
  button {
    padding: 0.4em 1em;
    border-radius: 8px;
    border: 1px solid #396cd8;
    background: #396cd8;
    color: #fff;
    cursor: pointer;
    font-size: 0.85em;
  }
  button:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
  button.ghost {
    background: transparent;
    border-color: #555;
  }
  button.danger {
    background: #8b2e2e;
    border-color: #8b2e2e;
  }
</style>
