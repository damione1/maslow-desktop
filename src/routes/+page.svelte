<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import {
    connection,
    fwVersion,
    connectWs,
    disconnectWs,
  } from "$lib/stores/connection";
  import {
    wsState,
    machineStatus,
    stateClass,
    initMachineListeners,
  } from "$lib/stores/machine";
  import { initJobListeners } from "$lib/stores/job";
  import { initMaslowListeners } from "$lib/stores/maslow";
  import { activeTab } from "$lib/stores/ui";
  import { layout } from "$lib/stores/viewport";
  import AppShell from "$lib/components/AppShell.svelte";
  import MobileShell from "$lib/components/MobileShell.svelte";
  import TabBar from "$lib/components/TabBar.svelte";
  import ConsoleDock from "$lib/components/ConsoleDock.svelte";
  import StatusPanel from "$lib/components/StatusPanel.svelte";
  import MaslowPanel from "$lib/components/MaslowPanel.svelte";
  import CalibrationWizard from "$lib/components/CalibrationWizard.svelte";
  import CalibrationView from "$lib/components/CalibrationView.svelte";
  import CalibrationSolver from "$lib/components/CalibrationSolver.svelte";
  import MaslowConfig from "$lib/components/MaslowConfig.svelte";
  import FluidNCConfig from "$lib/components/FluidNCConfig.svelte";
  import JogControls from "$lib/components/JogControls.svelte";
  import JobPanel from "$lib/components/JobPanel.svelte";
  import ToolpathView from "$lib/components/ToolpathView.svelte";

  let host = $state($connection.host);

  const connected = $derived($wsState === "connected");

  onMount(() => {
    initMachineListeners();
    initJobListeners();
    initMaslowListeners();
  });

  async function connect() {
    await connectWs(host);
  }

  async function disconnect() {
    await disconnectWs();
  }

  // 0x18 = Ctrl-X soft reset — the universal mid-cut kill. Always reachable from
  // the chrome, never gated behind a tab.
  function estop() {
    invoke("send_realtime", { byte: 0x18 });
  }
</script>

{#if $layout !== "desktop"}
  <MobileShell />
{:else}
<AppShell>
  {#snippet topbar()}
    <div class="topbar">
      <strong class="brand">Maslow Desktop</strong>
      <input
        class="host"
        placeholder="maslow.local or IP"
        bind:value={host}
        autocomplete="off"
        spellcheck="false"
      />
      {#if connected}
        <span class="badge ok">● Connected</span>
        {#if $fwVersion}
          <span class="fw" title="Maslow firmware version">FW {$fwVersion}</span>
        {/if}
        <button class="ghost" onclick={disconnect}>Disconnect</button>
      {:else}
        <span class="badge off">● Disconnected</span>
        <button onclick={connect}>Connect</button>
      {/if}

      {#if $machineStatus}
        <span class="state-pill {stateClass($machineStatus.state)}">
          {$machineStatus.state}{$machineStatus.substate !== null
            ? `:${$machineStatus.substate}`
            : ""}
        </span>
      {:else}
        <span class="state-pill other">—</span>
      {/if}

      <button
        class="estop"
        onclick={estop}
        disabled={!connected}
        title="Emergency stop — halts all motion immediately (realtime soft reset, Ctrl-X / 0x18). Recoverable: unlock or home afterward. Same as the rail's Reset."
      >
        ⛔ E-STOP
      </button>
    </div>
  {/snippet}

  {#snippet workspace()}
    <TabBar />
    <div class="panels">
      <!-- Inactive tabs are kept mounted and hidden with display:none (not
           {#if}) so the waypoint canvas keeps its drawing and FileBrowser /
           MaslowConfig don't re-fetch from the machine on every tab switch. -->
      <div class="panel" class:active={$activeTab === "job"}>
        <JobPanel />
        <ToolpathView />
      </div>
      <div class="panel" class:active={$activeTab === "calibrate"}>
        <!-- Controls first: the guided wizard and the contextual Maslow panel
             as two side-by-side columns. The waypoint "map" and the solver sit
             below — the map only fills out once calibration is running, so it
             stays out of the way at idle. -->
        <div class="cal-cols">
          <CalibrationWizard />
          <MaslowPanel />
        </div>
        <CalibrationView />
        <CalibrationSolver />
      </div>
      <div class="panel" class:active={$activeTab === "config"}>
        <MaslowConfig />
        <FluidNCConfig />
      </div>
    </div>
  {/snippet}

  {#snippet rail()}
    <div class="rail-inner">
      <StatusPanel />
      <JogControls />
    </div>
  {/snippet}

  {#snippet dock()}
    <ConsoleDock />
  {/snippet}
</AppShell>
{/if}

<style>
  :global(body) {
    margin: 0;
    font-family: Inter, Avenir, Helvetica, Arial, sans-serif;
    color: #f0f0f0;
    background: #1a1a1a;
  }

  .topbar {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0.5em 1em;
    background: #222;
    border-bottom: 1px solid #333;
  }
  .brand {
    margin-right: 0.5em;
    white-space: nowrap;
  }
  .host {
    flex: 0 0 200px;
    padding: 0.45em 0.8em;
    border-radius: 8px;
    border: 1px solid #444;
    background: #2b2b2b;
    color: #fff;
  }
  .badge {
    font-weight: 600;
    font-size: 0.9em;
    white-space: nowrap;
  }
  .badge.ok {
    color: #3ddc84;
  }
  .badge.off {
    color: #888;
  }
  .fw {
    font-size: 0.78em;
    color: #9bb4d8;
    background: #20304d;
    padding: 0.2em 0.55em;
    border-radius: 6px;
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }
  button {
    padding: 0.45em 1em;
    border-radius: 8px;
    border: 1px solid #396cd8;
    background: #396cd8;
    color: #fff;
    cursor: pointer;
    font-size: 0.9em;
  }
  button.ghost {
    background: transparent;
    border-color: #555;
  }

  .state-pill {
    font-weight: 700;
    font-size: 0.9em;
    padding: 0.25em 0.8em;
    border-radius: 6px;
    white-space: nowrap;
  }
  .state-pill.idle {
    background: #2e7d32;
  }
  .state-pill.run {
    background: #1565c0;
  }
  .state-pill.hold {
    background: #b8860b;
  }
  .state-pill.alarm {
    background: #c62828;
  }
  .state-pill.other {
    background: #444;
  }

  .estop {
    margin-left: auto;
    font-weight: 800;
    letter-spacing: 0.03em;
    background: #b71c1c;
    border-color: #b71c1c;
    white-space: nowrap;
  }
  .estop:hover:not(:disabled) {
    background: #d32f2f;
  }
  .estop:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .panels {
    flex: 1;
    min-height: 0;
    position: relative;
  }
  .panel {
    display: none;
    height: 100%;
    overflow: auto;
    flex-direction: column;
    gap: 14px;
    padding: 1em;
  }
  .panel.active {
    display: flex;
  }

  .rail-inner {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 0.6em;
  }

  /* Rail density pass — tightens the DRO + jog stack so it fits a ~756px-tall
     laptop window (default console dock) without a scrollbar, while keeping
     Hold/Resume/Reset reachable. Scoped to the rail via :global so the
     components themselves stay untouched (StatusPanel / JogControls render
     nowhere else). The rail still falls back to overflow-y:auto if the dock is
     dragged tall, so the realtime buttons are never clipped. */
  .rail-inner :global(.panel) {
    padding: 0.75em 1em;
  }
  .rail-inner :global(.panel .state) {
    margin-bottom: 0.5em;
  }
  .rail-inner :global(.panel .axis) {
    padding: 0.45em;
  }
  .rail-inner :global(.panel .wpos) {
    font-size: 1.4em;
  }
  .rail-inner :global(.panel .meta) {
    margin-top: 0.5em;
  }
  .rail-inner :global(.jog) {
    gap: 0.5em;
    padding: 0.6em 0.8em;
  }
  .rail-inner :global(.jog .xy) {
    grid-template-rows: repeat(3, 36px);
    gap: 5px;
  }
  .rail-inner :global(.jog .z) {
    grid-template-rows: 36px 36px;
  }

  /* Calibrate tab: wizard and Maslow panel side by side. minmax(0, …) lets the
     columns shrink so their inner grids/buttons never overflow; align-items:
     start keeps each column at its natural height instead of stretching the
     shorter one. */
  .cal-cols {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
    gap: 14px;
    align-items: start;
  }

  /* Match the shell's breakpoint: collapse to a single stacked column so
     neither panel gets crushed on a narrow screen. */
  @media (max-width: 820px) {
    .cal-cols {
      grid-template-columns: 1fr;
    }
    /* Stacked layout scrolls at the document level (see AppShell): drop the
       per-panel scroll container so we don't nest a second scrollbar. */
    .panel {
      height: auto;
      overflow: visible;
    }
  }
</style>
