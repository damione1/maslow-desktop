<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { wsState, machineStatus, stateClass } from "$lib/stores/machine";
  import {
    connection,
    fwVersion,
    connectWs,
    disconnectWs,
  } from "$lib/stores/connection";
  import { activeSection } from "$lib/stores/ui";
  import Modal from "./Modal.svelte";
  import MobileNav from "./MobileNav.svelte";
  import ControlView from "./ControlView.svelte";
  import JobPanel from "./JobPanel.svelte";
  import ToolpathView from "./ToolpathView.svelte";
  import CalibrationWizard from "./CalibrationWizard.svelte";
  import MaslowPanel from "./MaslowPanel.svelte";
  import MoreView from "./MoreView.svelte";

  const connected = $derived($wsState === "connected");

  let showConn = $state(false);
  let host = $state($connection.host);

  // 0x18 = Ctrl-X soft reset — the universal kill, always reachable from chrome.
  function stop() {
    invoke("send_realtime", { byte: 0x18 });
  }

  async function connect() {
    await connectWs(host);
    showConn = false;
  }
  async function disconnect() {
    await disconnectWs();
  }
</script>

<div class="mshell">
  <header class="bar">
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
      class="conn-chip"
      class:on={connected}
      onclick={() => (showConn = true)}
      title="Connection"
    >
      <span class="led"></span>
      {connected ? "Connected" : "Connect"}
    </button>

    <button class="stop" onclick={stop} disabled={!connected}>STOP</button>
  </header>

  <main class="content">
    <!-- Sections stay mounted (hidden) so the toolpath canvas keeps its drawing
         and JobPanel/FileBrowser don't refetch on every tab switch. -->
    <div class="section" class:active={$activeSection === "control"}>
      <ControlView />
    </div>
    <div class="section" class:active={$activeSection === "job"}>
      <JobPanel />
      <ToolpathView />
    </div>
    <div class="section" class:active={$activeSection === "calibrate"}>
      <CalibrationWizard />
      <MaslowPanel />
    </div>
    <div class="section" class:active={$activeSection === "more"}>
      <MoreView />
    </div>
  </main>

  <MobileNav />
</div>

{#if showConn}
  <Modal title="Connection" onclose={() => (showConn = false)}>
    <input
      class="conn-host"
      placeholder="maslow.local or IP"
      bind:value={host}
      autocomplete="off"
      spellcheck="false"
    />
    <div class="conn-row">
      {#if connected}
        <span class="conn-state ok">● Connected{$fwVersion ? ` · FW ${$fwVersion}` : ""}</span>
        <button class="ghost" onclick={disconnect}>Disconnect</button>
      {:else}
        <span class="conn-state off">● Disconnected</span>
        <button onclick={connect}>Connect</button>
      {/if}
    </div>
  </Modal>
{/if}

<style>
  .mshell {
    display: flex;
    flex-direction: column;
    height: 100vh;
    height: 100dvh;
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 0.6em;
    padding: 0.5em 0.7em;
    padding-top: calc(0.5em + env(safe-area-inset-top, 0));
    background: #1d1d1d;
    border-bottom: 1px solid #2f2f2f;
    flex: 0 0 auto;
  }
  .state-pill {
    font-weight: 700;
    font-size: 0.82em;
    padding: 0.3em 0.7em;
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
  .conn-chip {
    display: inline-flex;
    align-items: center;
    gap: 0.45em;
    padding: 0.4em 0.7em;
    min-height: 40px;
    border-radius: 8px;
    border: 1px solid #444;
    background: #262626;
    color: #ddd;
    font: inherit;
    font-size: 0.82em;
    cursor: pointer;
  }
  .conn-chip .led {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: #888;
  }
  .conn-chip.on .led {
    background: #3ddc84;
  }
  .stop {
    margin-left: auto;
    min-width: 88px;
    min-height: 44px;
    font-weight: 800;
    letter-spacing: 0.05em;
    font-size: 0.95em;
    border-radius: 8px;
    border: 1px solid #b71c1c;
    background: #b71c1c;
    color: #fff;
    cursor: pointer;
  }
  .stop:disabled {
    opacity: 0.4;
  }
  .content {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    -webkit-overflow-scrolling: touch;
  }
  .section {
    display: none;
    flex-direction: column;
    gap: 12px;
    padding: 0.8em;
  }
  .section.active {
    display: flex;
  }

  .conn-host {
    width: 100%;
    box-sizing: border-box;
    padding: 0.6em 0.8em;
    min-height: 44px;
    border-radius: 8px;
    border: 1px solid #444;
    background: #2b2b2b;
    color: #fff;
    margin-bottom: 0.8em;
  }
  .conn-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.8em;
  }
  .conn-state {
    font-size: 0.85em;
    font-weight: 600;
  }
  .conn-state.ok {
    color: #3ddc84;
  }
  .conn-state.off {
    color: #888;
  }
  .conn-row button {
    padding: 0.55em 1.2em;
    min-height: 44px;
    border-radius: 8px;
    border: 1px solid #396cd8;
    background: #396cd8;
    color: #fff;
    cursor: pointer;
  }
  .conn-row button.ghost {
    background: transparent;
    border-color: #555;
  }
</style>
