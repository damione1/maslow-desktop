<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { wsState } from "$lib/stores/machine";
  import { connection } from "$lib/stores/connection";
  import { jobProgress } from "$lib/stores/job";
  import {
    fullConfig,
    refreshFullConfig,
    type ConfigEntry,
  } from "$lib/stores/maslow";

  const connected = $derived($wsState === "connected");
  const jobActive = $derived(
    $jobProgress?.state === "running" || $jobProgress?.state === "paused",
  );
  const editable = $derived(connected && !jobActive);

  let busy = $state(false);
  let message = $state("");
  let error = $state("");
  let filter = $state("");
  // Edited values keyed by path; only paths present here are written.
  let draft = $state<Record<string, string>>({});

  async function load() {
    busy = true;
    error = "";
    message = "";
    try {
      await refreshFullConfig();
      draft = {};
    } catch (e) {
      error = `Read failed: ${e}`;
    } finally {
      busy = false;
    }
  }

  // Group entries by their top-level section (first path segment, or "general"
  // for bare root keys), filtered by the search box.
  const groups = $derived.by(() => {
    const cfg = $fullConfig ?? [];
    const q = filter.trim().toLowerCase();
    const map = new Map<string, ConfigEntry[]>();
    for (const e of cfg) {
      if (q && !e.path.toLowerCase().includes(q)) continue;
      const slash = e.path.indexOf("/");
      const section = slash === -1 ? "general" : e.path.slice(0, slash);
      const list = map.get(section) ?? [];
      list.push(e);
      map.set(section, list);
    }
    return [...map.entries()].sort((a, b) => a[0].localeCompare(b[0]));
  });

  const current = (e: ConfigEntry) => draft[e.path] ?? e.value;
  const isDirty = (e: ConfigEntry) =>
    draft[e.path] !== undefined && draft[e.path] !== e.value;
  const dirtyCount = $derived(
    ($fullConfig ?? []).filter((e) => isDirty(e)).length,
  );

  function set(path: string, value: string) {
    draft = { ...draft, [path]: value };
  }

  // Shorten the path for display: show the trailing segment, full path on hover.
  const leaf = (path: string) => {
    const i = path.lastIndexOf("/");
    return i === -1 ? path : path.slice(i + 1);
  };

  async function save() {
    const cfg = $fullConfig;
    if (!cfg) return;
    busy = true;
    error = "";
    message = "";
    const host = $connection.host;
    try {
      let written = 0;
      for (const e of cfg) {
        if (isDirty(e)) {
          await invoke("write_maslow_setting", {
            host,
            path: e.path,
            value: draft[e.path],
          });
          written++;
        }
      }
      if (written > 0) {
        await invoke("save_maslow_config", { host });
      }
      message = written > 0 ? `Wrote ${written} setting(s) to flash.` : "No changes.";
      await load();
    } catch (e) {
      error = `Write failed: ${e}`;
    } finally {
      busy = false;
    }
  }

  function revert() {
    draft = {};
    message = "";
    error = "";
  }
</script>

<section class="fc">
  <header>
    <span>FluidNC config</span>
    <span class="sub">full machine tree (<code>$CD</code>)</span>
    <button class="ghost" onclick={load} disabled={!connected || busy}>
      {busy ? "…" : $fullConfig ? "Reload" : "Read"}
    </button>
  </header>

  {#if !$fullConfig}
    <div class="hint">
      {connected
        ? busy
          ? "dumping configuration…"
          : "press Read to dump the full FluidNC config"
        : "connect to read/edit the configuration"}
    </div>
  {:else}
    <div class="warn">
      ⚠ This is the entire machine config, including wiring (pins, motors,
      stepping). Editing the wrong field can make the machine unbootable. Change
      only what you understand; values are written to flash on Save.
    </div>

    <div class="toolbar">
      <input
        class="search"
        type="text"
        placeholder="filter by path…"
        bind:value={filter}
      />
      {#if dirtyCount > 0}
        <span class="dirty">{dirtyCount} changed</span>
      {/if}
      <button class="save" onclick={save} disabled={!editable || busy || dirtyCount === 0}>
        Save
      </button>
      <button class="ghost" onclick={revert} disabled={busy || dirtyCount === 0}>
        Revert
      </button>
    </div>

    {#if !editable}
      <div class="note">
        {jobActive ? "read-only while a job is running" : "connect to edit"}
      </div>
    {/if}

    <div class="groups">
      {#each groups as [section, entries]}
        <details open={!!filter || entries.length <= 12}>
          <summary>{section} <span class="n">{entries.length}</span></summary>
          <div class="fields">
            {#each entries as e}
              <label class="field" class:dirty={isDirty(e)} title={e.path}>
                <span class="lab">{leaf(e.path)}</span>
                {#if e.kind === "bool"}
                  <input
                    type="checkbox"
                    checked={current(e) === "true"}
                    disabled={!editable}
                    onchange={(ev) =>
                      set(e.path, ev.currentTarget.checked ? "true" : "false")}
                  />
                {:else}
                  <input
                    type={e.kind === "int" || e.kind === "float" ? "number" : "text"}
                    step={e.kind === "float" ? "any" : e.kind === "int" ? "1" : undefined}
                    value={current(e)}
                    disabled={!editable}
                    oninput={(ev) => set(e.path, ev.currentTarget.value)}
                  />
                {/if}
              </label>
            {/each}
          </div>
        </details>
      {/each}
    </div>

    {#if message}<div class="msg ok">{message}</div>{/if}
    {#if error}<div class="msg err">{error}</div>{/if}
  {/if}
</section>

<style>
  .fc {
    border: 1px solid #2a2a2a;
    border-radius: 8px;
    background: #1a1a1a;
    padding: 0.75em 0.9em 0.9em;
    display: flex;
    flex-direction: column;
    gap: 0.6em;
    font-size: 0.85em;
  }
  header {
    display: flex;
    align-items: baseline;
    gap: 0.6em;
  }
  header > span:first-child {
    font-weight: 600;
  }
  .sub {
    color: #888;
    font-size: 0.85em;
  }
  .sub code {
    background: #262626;
    padding: 0 0.3em;
    border-radius: 3px;
  }
  header .ghost {
    margin-left: auto;
  }
  .hint,
  .note {
    color: #9a9a9a;
  }
  .warn {
    color: #d8a657;
    background: #2a2317;
    border: 1px solid #4a3d1f;
    border-radius: 5px;
    padding: 0.45em 0.6em;
    line-height: 1.35;
  }
  .toolbar {
    display: flex;
    align-items: center;
    gap: 0.5em;
  }
  .search {
    flex: 1;
    min-width: 0;
    background: #111;
    border: 1px solid #333;
    border-radius: 4px;
    color: #ddd;
    padding: 0.3em 0.5em;
  }
  .dirty {
    color: #d8a657;
    white-space: nowrap;
  }
  button {
    border-radius: 5px;
    padding: 0.32em 0.7em;
    cursor: pointer;
    border: 1px solid #444;
    background: #2a2a2a;
    color: #ddd;
  }
  button.save {
    background: #2f6f4f;
    border-color: #2f6f4f;
    color: #fff;
  }
  button:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .groups {
    display: flex;
    flex-direction: column;
    gap: 0.3em;
  }
  details {
    border: 1px solid #242424;
    border-radius: 5px;
    background: #161616;
  }
  summary {
    cursor: pointer;
    padding: 0.4em 0.6em;
    font-weight: 500;
    user-select: none;
  }
  summary .n {
    color: #777;
    font-weight: 400;
  }
  .fields {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.25em 0.8em;
    padding: 0.3em 0.6em 0.6em;
  }
  .field {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5em;
    min-width: 0;
  }
  .field .lab {
    color: #b0b0b0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .field.dirty .lab {
    color: #d8a657;
  }
  .field input[type="text"],
  .field input[type="number"] {
    width: 9em;
    background: #111;
    border: 1px solid #333;
    border-radius: 4px;
    color: #ddd;
    padding: 0.2em 0.4em;
    font-variant-numeric: tabular-nums;
  }
  .field.dirty input {
    border-color: #d8a657;
  }
  .msg.ok {
    color: #a9b665;
  }
  .msg.err {
    color: #ea6962;
  }
  @media (max-width: 820px) {
    .fields {
      grid-template-columns: 1fr;
    }
  }
</style>
