<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { connection } from "$lib/stores/connection";
  import { wsState } from "$lib/stores/machine";
  import { jobProgress, loadSavedJob, loadToolpath, type SavedJob } from "$lib/stores/job";
  import Modal from "./Modal.svelte";
  import FileBrowser from "./FileBrowser.svelte";

  const GCODE_FILTER = {
    name: "G-code",
    extensions: ["nc", "gcode", "gc", "ngc", "tap", "cnc", "txt"],
  };

  let filePath = $state<string | null>(null);
  let fileName = $state<string>("");
  let saved = $state<SavedJob | null>(null);
  let busy = $state(false);
  let notice = $state<string>("");
  let showSd = $state(false);

  const connected = $derived($wsState === "connected");
  const job = $derived($jobProgress);
  const active = $derived(
    job !== null && job.state !== "done" && job.state !== "error",
  );
  const percent = $derived(
    job && job.total > 0 ? Math.round((job.acked / job.total) * 100) : 0,
  );

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

  async function chooseGcode() {
    const p = await pickFile();
    if (p) {
      filePath = p;
      fileName = basename(p);
      notice = "";
      loadToolpath(p);
    }
  }

  async function start() {
    if (!filePath) return;
    busy = true;
    try {
      const total = await invoke<number>("stream_start", {
        path: filePath,
        startIndex: 0,
      });
      notice = `Streaming ${total} lines`;
    } catch (e) {
      notice = `Error: ${e}`;
    } finally {
      busy = false;
    }
  }

  async function resumeSaved() {
    if (!saved) return;
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

  const pause = () => invoke("stream_pause");
  const resume = () => invoke("stream_resume");
  const stop = () => invoke("stream_stop");

  async function uploadToSd() {
    const p = await pickFile();
    if (!p) return;
    busy = true;
    notice = `Uploading ${basename(p)}…`;
    try {
      await invoke("upload_file", {
        host: $connection.host,
        dir: "/",
        localPath: p,
      });
      notice = `Uploaded ${basename(p)} to /SD`;
    } catch (e) {
      notice = `Upload error: ${e}`;
    } finally {
      busy = false;
    }
  }
</script>

<section class="job">
  <header>
    <span>G-code Job</span>
    {#if notice}<span class="notice">{notice}</span>{/if}
  </header>

  {#if saved && !active}
    <div class="resume-banner">
      <span>
        Interrupted job: <strong>{saved.name}</strong>
        at line {saved.acked}/{saved.total}
      </span>
      <div class="row">
        <button onclick={resumeSaved} disabled={!connected || busy}>Resume</button>
        <button class="ghost" onclick={() => (saved = null)}>Dismiss</button>
      </div>
    </div>
  {/if}

  <div class="picker">
    <button class="ghost" onclick={chooseGcode} disabled={active || busy}>
      Open G-code…
    </button>
    <span class="filename">{fileName || "no file selected"}</span>
    <button class="ghost" onclick={() => (showSd = true)} disabled={!connected}>
      SD Files…
    </button>
    <button onclick={uploadToSd} disabled={!connected || busy} class="ghost">
      Upload to SD
    </button>
  </div>

  {#if job}
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
  {/if}

  <div class="controls">
    {#if active}
      {#if job?.state === "running"}
        <button onclick={pause} disabled={!connected}>Pause</button>
      {:else}
        <button onclick={resume} disabled={!connected}>Resume</button>
      {/if}
      <button class="danger" onclick={stop}>Stop</button>
    {:else}
      <button onclick={start} disabled={!connected || !filePath || busy}>
        Start
      </button>
    {/if}
  </div>
</section>

{#if showSd}
  <Modal title="SD Files" onclose={() => (showSd = false)}>
    <FileBrowser />
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
  .picker {
    display: flex;
    align-items: center;
    gap: 0.6em;
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
