<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { connection } from "$lib/stores/connection";
  import { wsState } from "$lib/stores/machine";
  import { jobProgress, loadSdToolpath } from "$lib/stores/job";

  // Called after a successful preview so the parent (e.g. the SD modal) can
  // close and reveal the toolpath underneath.
  let { onpreview }: { onpreview?: () => void } = $props();

  interface SdEntry {
    name: string;
    size: string | number;
    isdir?: boolean;
  }

  let path = $state("/");
  let entries = $state<SdEntry[]>([]);
  let loading = $state(false);
  let error = $state("");

  const connected = $derived($wsState === "connected");
  const jobActive = $derived(
    $jobProgress !== null &&
      $jobProgress.state !== "done" &&
      $jobProgress.state !== "error",
  );

  function isDir(e: SdEntry): boolean {
    return e.isdir === true || String(e.size) === "-1";
  }

  function humanSize(e: SdEntry): string {
    if (isDir(e)) return "dir";
    const n = Number(e.size);
    if (!isFinite(n)) return String(e.size);
    if (n < 1024) return `${n} B`;
    if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
    return `${(n / 1024 / 1024).toFixed(1)} MB`;
  }

  function fullPath(name: string): string {
    return path.endsWith("/") ? `${path}${name}` : `${path}/${name}`;
  }

  async function refresh() {
    if (!connected) return;
    loading = true;
    error = "";
    try {
      const res = await invoke<{ files?: SdEntry[] }>("list_files", {
        host: $connection.host,
        path,
      });
      entries = res.files ?? [];
    } catch (e) {
      error = String(e);
      entries = [];
    } finally {
      loading = false;
    }
  }

  function enterDir(name: string) {
    path = fullPath(name);
    refresh();
  }

  function goUp() {
    if (path === "/") return;
    const trimmed = path.replace(/\/$/, "");
    const parent = trimmed.slice(0, trimmed.lastIndexOf("/"));
    path = parent === "" ? "/" : parent;
    refresh();
  }

  async function del(name: string) {
    try {
      await invoke("delete_file", {
        host: $connection.host,
        dir: path,
        filename: name,
      });
      refresh();
    } catch (e) {
      error = String(e);
    }
  }

  function run(name: string) {
    // Firmware-side run from SD (not the client streamer).
    invoke("send_line", { line: `$SD/Run=${fullPath(name)}` });
  }

  async function preview(name: string) {
    error = "";
    try {
      await loadSdToolpath($connection.host, fullPath(name));
      onpreview?.();
    } catch (e) {
      error = String(e);
    }
  }

  $effect(() => {
    if (connected && entries.length === 0 && !loading && !error) {
      refresh();
    }
  });
</script>

<section class="files">
  <header>
    <span>SD Files</span>
    <span class="path">{path}</span>
    <button class="ghost sm" onclick={refresh} disabled={!connected || loading}>
      {loading ? "…" : "Refresh"}
    </button>
  </header>

  {#if error}
    <div class="error">{error}</div>
  {/if}

  <div class="list">
    {#if path !== "/"}
      <button class="entry up" onclick={goUp}>↑ ..</button>
    {/if}
    {#each entries as e}
      <div class="entry" class:dir={isDir(e)}>
        {#if isDir(e)}
          <button class="name" onclick={() => enterDir(e.name)}>📁 {e.name}</button>
        {:else}
          <span class="name">{e.name}</span>
        {/if}
        <span class="size">{humanSize(e)}</span>
        {#if !isDir(e)}
          <button class="sm" onclick={() => preview(e.name)}>Preview</button>
          <button class="sm" onclick={() => run(e.name)} disabled={jobActive}>Run</button>
          <button class="sm danger" onclick={() => del(e.name)}>✕</button>
        {/if}
      </div>
    {/each}
    {#if connected && entries.length === 0 && !loading}
      <div class="empty">empty</div>
    {/if}
    {#if !connected}
      <div class="empty">connect to browse the SD card</div>
    {/if}
  </div>
</section>

<style>
  .files {
    background: #161616;
    border: 1px solid #333;
    border-radius: 10px;
    padding: 0.7em 0.9em;
    display: flex;
    flex-direction: column;
    gap: 0.5em;
  }
  header {
    display: flex;
    align-items: center;
    gap: 0.7em;
    font-size: 0.85em;
    opacity: 0.85;
  }
  .path {
    flex: 1;
    font-family: "SF Mono", Menlo, Consolas, monospace;
    font-size: 0.8em;
    opacity: 0.6;
  }
  .error {
    font-size: 0.8em;
    color: #ff6b6b;
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-height: 220px;
    overflow-y: auto;
  }
  .entry {
    display: flex;
    align-items: center;
    gap: 0.6em;
    padding: 0.3em 0.4em;
    border-radius: 6px;
    font-size: 0.85em;
  }
  .entry:hover {
    background: #1f1f1f;
  }
  .entry.up {
    border: none;
    background: transparent;
    color: #aaa;
    text-align: left;
    cursor: pointer;
  }
  .name {
    flex: 1;
    text-align: left;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    background: none;
    border: none;
    color: inherit;
    font: inherit;
    cursor: default;
    padding: 0;
  }
  .entry.dir .name {
    cursor: pointer;
    color: #7fb2ff;
  }
  .size {
    font-size: 0.78em;
    opacity: 0.55;
    font-variant-numeric: tabular-nums;
    min-width: 56px;
    text-align: right;
  }
  .empty {
    font-size: 0.8em;
    opacity: 0.5;
    padding: 0.4em;
  }
  button.sm {
    padding: 0.2em 0.6em;
    border-radius: 6px;
    border: 1px solid #444;
    background: #2b2b2b;
    color: #ddd;
    cursor: pointer;
    font-size: 0.78em;
  }
  button.sm:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
  .ghost {
    background: transparent;
    border: 1px solid #555;
    color: #ddd;
    border-radius: 7px;
    cursor: pointer;
  }
  .danger {
    background: #6e2525;
    border-color: #6e2525;
    color: #fff;
  }
</style>
