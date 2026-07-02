<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { connection } from "$lib/stores/connection";
  import { wsState } from "$lib/stores/machine";
  import { loadSdToolpath } from "$lib/stores/job";
  import { confirmDialog } from "$lib/stores/confirm";

  const GCODE_FILTER = {
    name: "G-code",
    extensions: ["nc", "gcode", "gc", "ngc", "tap", "cnc", "txt"],
  };

  // Called after the operator picks an SD file: the parent (Job panel) records
  // it as the loaded job and closes the modal. Running happens from the Job
  // zone's single Start button, not from here, so the two job sources (local
  // stream vs SD card) share one launch path.
  let { onselect }: { onselect?: (f: { path: string; name: string }) => void } =
    $props();

  interface SdEntry {
    name: string;
    size: string | number;
    isdir?: boolean;
  }

  let path = $state("/");
  let entries = $state<SdEntry[]>([]);
  let loading = $state(false);
  let uploading = $state(false);
  let error = $state("");

  const connected = $derived($wsState === "connected");

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
    if (
      !(await confirmDialog(`Delete ${name} from the SD card? This cannot be undone.`, {
        danger: true,
      }))
    )
      return;
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

  // Pick an SD file as the loaded job: parse its toolpath for preview and hand
  // the path back to the Job panel. The actual run is launched there.
  async function select(name: string) {
    error = "";
    try {
      await loadSdToolpath($connection.host, fullPath(name));
      onselect?.({ path: fullPath(name), name });
    } catch (e) {
      error = String(e);
    }
  }

  // Upload a local file into the directory currently being browsed, then load
  // it as the active job (preview, not run) so the operator lands back in the
  // Job zone with it selected.
  async function upload() {
    const sel = await open({
      multiple: false,
      directory: false,
      filters: [GCODE_FILTER],
    });
    if (typeof sel !== "string") return;
    const name = sel.split(/[\\/]/).pop() ?? sel;
    uploading = true;
    error = "";
    try {
      await invoke("upload_file", {
        host: $connection.host,
        dir: path,
        localPath: sel,
      });
      await select(name);
    } catch (e) {
      error = String(e);
    } finally {
      uploading = false;
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
    <button class="ghost sm" onclick={upload} disabled={!connected || uploading}>
      {uploading ? "Uploading…" : "Upload…"}
    </button>
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
          <button class="sm primary" onclick={() => select(e.name)}>Select</button>
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
    background: var(--surface);
    border: 1px solid var(--border-2);
    border-radius: var(--radius-lg);
    padding: 0.7em 0.9em;
    display: flex;
    flex-direction: column;
    gap: 0.6em;
    min-height: 0;
    height: 100%;
  }
  header {
    display: flex;
    align-items: center;
    gap: 0.7em;
    font-size: 0.95em;
    color: var(--text-dim);
    flex: 0 0 auto;
  }
  .path {
    flex: 1;
    font-family: var(--mono);
    font-size: 0.85em;
    color: var(--text-mute);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .error {
    font-size: 0.85em;
    color: #ff6b6b;
    flex: 0 0 auto;
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 4px;
    flex: 1 1 auto;
    min-height: 0;
    overflow-y: auto;
  }
  .entry {
    display: flex;
    align-items: center;
    gap: 0.7em;
    min-height: var(--tap);
    padding: 0.4em 0.7em;
    border-radius: var(--radius);
    font-size: 1em;
    background: var(--surface-2);
  }
  .entry:hover {
    background: var(--surface-3);
  }
  .entry.up {
    border: none;
    background: var(--surface-2);
    color: var(--text-dim);
    text-align: left;
    cursor: pointer;
    min-height: var(--tap);
    padding: 0.4em 0.7em;
    border-radius: var(--radius);
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
    font-size: 0.85em;
    color: var(--text-mute);
    font-variant-numeric: tabular-nums;
    min-width: 64px;
    text-align: right;
  }
  .empty {
    font-size: 0.9em;
    color: var(--text-mute);
    padding: 0.6em;
  }
  button.sm {
    min-height: calc(var(--tap) - 8px);
    padding: 0 0.9em;
    border-radius: var(--radius);
    border: 1px solid var(--border-3);
    background: var(--surface-3);
    color: var(--text);
    cursor: pointer;
    font-size: 0.9em;
    font-weight: 600;
  }
  button.sm:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
  button.sm.primary {
    border-color: var(--action);
    background: var(--action);
    color: #fff;
  }
  .ghost {
    min-height: calc(var(--tap) - 8px);
    padding: 0 0.9em;
    background: transparent;
    border: 1px solid var(--border-3);
    color: var(--text);
    border-radius: var(--radius);
    cursor: pointer;
    font-size: 0.9em;
  }
  .ghost:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
  .danger {
    background: var(--danger);
    border-color: var(--danger);
    color: #fff;
  }
</style>
