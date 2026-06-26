<script lang="ts">
  import {
    connection,
    fwVersion,
    connectWs,
    disconnectWs,
  } from "$lib/stores/connection";
  import { wsState } from "$lib/stores/machine";
  import MaslowConfig from "./MaslowConfig.svelte";
  import FluidNCConfig from "./FluidNCConfig.svelte";
  import Console from "./Console.svelte";

  let host = $state($connection.host);
  const connected = $derived($wsState === "connected");

  async function connect() {
    await connectWs(host);
  }
  async function disconnect() {
    await disconnectWs();
  }
</script>

<div class="more">
  <section class="card conn">
    <header>Connection</header>
    <input
      class="host"
      placeholder="maslow.local or IP"
      bind:value={host}
      autocomplete="off"
      spellcheck="false"
    />
    <div class="row">
      {#if connected}
        <span class="badge ok">● Connected</span>
        {#if $fwVersion}<span class="fw">FW {$fwVersion}</span>{/if}
        <button class="ghost" onclick={disconnect}>Disconnect</button>
      {:else}
        <span class="badge off">● Disconnected</span>
        <button onclick={connect}>Connect</button>
      {/if}
    </div>
  </section>

  <details class="card">
    <summary>Machine &amp; firmware settings</summary>
    <div class="disclosure">
      <MaslowConfig />
      <FluidNCConfig />
    </div>
  </details>

  <details class="card">
    <summary>Console</summary>
    <div class="disclosure console-wrap">
      <Console />
    </div>
  </details>
</div>

<style>
  .more {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .card {
    background: #161616;
    border: 1px solid #333;
    border-radius: 10px;
    padding: 0.8em 0.9em;
  }
  .conn {
    display: flex;
    flex-direction: column;
    gap: 0.7em;
  }
  header {
    font-size: 0.85em;
    opacity: 0.85;
    font-weight: 600;
  }
  .host {
    padding: 0.6em 0.8em;
    border-radius: 8px;
    border: 1px solid #444;
    background: #2b2b2b;
    color: #fff;
    min-height: 44px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 0.8em;
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
  .fw {
    font-size: 0.78em;
    color: #9bb4d8;
    background: #20304d;
    padding: 0.2em 0.55em;
    border-radius: 6px;
    font-variant-numeric: tabular-nums;
  }
  button {
    margin-left: auto;
    padding: 0.55em 1.2em;
    min-height: 44px;
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
  summary {
    cursor: pointer;
    font-size: 0.9em;
    font-weight: 600;
    opacity: 0.9;
    user-select: none;
  }
  .disclosure {
    display: flex;
    flex-direction: column;
    gap: 12px;
    margin-top: 0.8em;
  }
  .console-wrap {
    height: 320px;
  }
</style>
