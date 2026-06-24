<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { connection, persistHost } from "$lib/stores/connection";
  import { wsState, initMachineListeners } from "$lib/stores/machine";
  import { initJobListeners } from "$lib/stores/job";
  import { initMaslowListeners } from "$lib/stores/maslow";
  import StatusPanel from "$lib/components/StatusPanel.svelte";
  import MaslowPanel from "$lib/components/MaslowPanel.svelte";
  import JogControls from "$lib/components/JogControls.svelte";
  import JobPanel from "$lib/components/JobPanel.svelte";
  import FileBrowser from "$lib/components/FileBrowser.svelte";
  import Console from "$lib/components/Console.svelte";

  let host = $state($connection.host);

  onMount(() => {
    initMachineListeners();
    initJobListeners();
    initMaslowListeners();
  });

  async function connect() {
    persistHost(host);
    connection.update((c) => ({ ...c, host }));
    await invoke("connect_ws", { host });
  }

  async function disconnect() {
    await invoke("disconnect_ws");
  }
</script>

<div class="app">
  <header class="topbar">
    <strong class="brand">Maslow Desktop</strong>
    <input
      class="host"
      placeholder="maslow.local or IP"
      bind:value={host}
      autocomplete="off"
      spellcheck="false"
    />
    {#if $wsState === "connected"}
      <span class="badge ok">● Connected</span>
      <button class="ghost" onclick={disconnect}>Disconnect</button>
    {:else}
      <span class="badge off">● Disconnected</span>
      <button onclick={connect}>Connect</button>
    {/if}
  </header>

  <main class="content">
    <div class="cols">
      <div class="col">
        <StatusPanel />
        <MaslowPanel />
        <JogControls />
      </div>
      <div class="col">
        <JobPanel />
        <FileBrowser />
      </div>
    </div>
    <Console />
  </main>
</div>

<style>
  :global(body) {
    margin: 0;
    font-family: Inter, Avenir, Helvetica, Arial, sans-serif;
    color: #f0f0f0;
    background: #1a1a1a;
  }
  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }
  .topbar {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0.6em 1em;
    background: #222;
    border-bottom: 1px solid #333;
  }
  .brand {
    margin-right: 0.5em;
  }
  .host {
    flex: 0 0 220px;
    padding: 0.45em 0.8em;
    border-radius: 8px;
    border: 1px solid #444;
    background: #2b2b2b;
    color: #fff;
  }
  .badge {
    font-weight: 600;
    font-size: 0.9em;
  }
  .badge.ok {
    color: #3ddc84;
  }
  .badge.off {
    color: #888;
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
  .content {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 1em;
    overflow: auto;
  }
  .cols {
    display: grid;
    grid-template-columns: minmax(320px, 1fr) minmax(320px, 1fr);
    gap: 14px;
    align-items: start;
  }
  .col {
    display: flex;
    flex-direction: column;
    gap: 14px;
    min-width: 0;
  }
  @media (max-width: 720px) {
    .cols {
      grid-template-columns: 1fr;
    }
  }
</style>
