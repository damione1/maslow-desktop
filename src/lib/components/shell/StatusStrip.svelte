<script lang="ts">
  import { onMount } from "svelte";
  import { connection, fwVersion } from "$lib/stores/connection";
  import { firmwareNotice } from "$lib/stores/firmware";
  import { machineStatus, wsState, stateClass } from "$lib/stores/machine";

  const connected = $derived($wsState === "connected");
  const s = $derived($machineStatus);
  const feedOv = $derived(s?.ov?.[0]);
  const stateLabel = $derived(
    s ? `${s.state}${s.substate !== null ? `:${s.substate}` : ""}` : connected ? "—" : "Offline",
  );
  const stateCls = $derived(s ? stateClass(s.state) : "other");

  // App version (from tauri.conf.json) for the footer; best-effort.
  let appVersion = $state<string | null>(null);
  onMount(async () => {
    try {
      const { getVersion } = await import("@tauri-apps/api/app");
      appVersion = await getVersion();
    } catch {
      appVersion = null;
    }
  });
</script>

<div class="strip">
  <span class="state-pill {stateCls}">{stateLabel}</span>

  <span class="item">
    <span class="k">Conn</span>
    <span class="v" class:ok={connected}>{connected ? $connection.host : "offline"}</span>
  </span>

  <span class="item">
    <span class="k">Units</span>
    <span class="v">MM</span>
  </span>

  {#if feedOv !== undefined}
    <span class="item">
      <span class="k">Feed</span>
      <span class="v">{feedOv}%</span>
    </span>
  {/if}

  {#if $fwVersion}
    <span class="item" title={$firmwareNotice ?? "Maslow firmware version"}>
      <span class="k">FW</span>
      <span class="v" class:warn={$firmwareNotice}>{$fwVersion}</span>
    </span>
  {/if}

  {#if appVersion}
    <span class="item">
      <span class="k">App</span>
      <span class="v">{appVersion}</span>
    </span>
  {/if}

  {#if $firmwareNotice}
    <span class="item notice" title={$firmwareNotice}>⚠ Firmware untested</span>
  {/if}
</div>

<style>
  .strip {
    display: flex;
    align-items: center;
    gap: var(--gap);
    padding: 4px 12px;
    min-height: 30px;
    background: var(--bar);
    border-top: 1px solid var(--border-2);
    font-size: 0.8em;
    color: var(--text-dim);
    overflow-x: auto;
    white-space: nowrap;
  }
  .item {
    display: inline-flex;
    align-items: center;
    gap: 0.4em;
  }
  .k {
    color: var(--text-mute);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    font-size: 0.92em;
  }
  .v {
    font-family: var(--mono);
    color: var(--text);
  }
  .v.ok {
    color: #3ddc84;
  }
  .v.warn {
    color: var(--warn);
  }
  /* Prominent machine-state pill — the at-a-glance state indicator now that the
     full-width bar is gone from MAIN. */
  .state-pill {
    font-weight: 700;
    font-size: 0.95em;
    padding: 0.15em 0.7em;
    border-radius: var(--radius);
    color: #fff;
    background: var(--state-other);
  }
  .state-pill.idle {
    background: var(--state-idle);
  }
  .state-pill.run {
    background: var(--state-run);
  }
  .state-pill.hold {
    background: var(--state-hold);
  }
  .state-pill.alarm {
    background: var(--state-alarm);
  }
  .notice {
    color: var(--warn);
    margin-left: auto;
  }
</style>
