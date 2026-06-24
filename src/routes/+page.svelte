<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { connection, persistHost } from "$lib/stores/connection";

  interface PingResult {
    reachable: boolean;
    status: number;
    info: string;
  }

  let host = $state($connection.host);

  async function testConnection(event: Event) {
    event.preventDefault();
    persistHost(host);
    connection.update((c) => ({ ...c, host, state: "testing", error: "" }));
    try {
      const res = await invoke<PingResult>("ping_machine", { host });
      if (res.reachable) {
        connection.set({ host, state: "connected", info: res.info, error: "" });
      } else {
        connection.set({
          host,
          state: "disconnected",
          info: "",
          error: res.info || `HTTP ${res.status}`,
        });
      }
    } catch (e) {
      connection.set({ host, state: "disconnected", info: "", error: String(e) });
    }
  }
</script>

<main class="container">
  <h1>Maslow Desktop</h1>
  <p class="subtitle">Connexion à la machine</p>

  <form class="row" onsubmit={testConnection}>
    <input
      placeholder="maslow.local ou IP"
      bind:value={host}
      autocomplete="off"
      spellcheck="false"
    />
    <button type="submit" disabled={$connection.state === "testing"}>
      {$connection.state === "testing" ? "Test…" : "Tester"}
    </button>
  </form>

  <div class="status">
    {#if $connection.state === "connected"}
      <span class="badge ok">● Connecté</span> à <strong>{$connection.host}</strong>
    {:else if $connection.state === "testing"}
      <span class="badge wait">● Test en cours…</span>
    {:else}
      <span class="badge off">● Déconnecté</span>
      {#if $connection.error}
        <div class="error">{$connection.error}</div>
      {/if}
    {/if}
  </div>

  {#if $connection.state === "connected" && $connection.info}
    <pre class="info">{$connection.info}</pre>
  {/if}
</main>

<style>
  :root {
    font-family: Inter, Avenir, Helvetica, Arial, sans-serif;
    color: #f6f6f6;
    background-color: #1f1f1f;
  }
  .container {
    margin: 0 auto;
    padding-top: 8vh;
    max-width: 640px;
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
  }
  .subtitle {
    opacity: 0.7;
    margin-top: -0.5em;
  }
  .row {
    display: flex;
    gap: 8px;
    margin: 1.5em 0;
  }
  input,
  button {
    border-radius: 8px;
    border: 1px solid #444;
    padding: 0.6em 1.2em;
    font-size: 1em;
    font-family: inherit;
    color: #fff;
    background-color: #2b2b2b;
  }
  input {
    min-width: 260px;
  }
  button {
    cursor: pointer;
    border-color: #396cd8;
  }
  button:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .status {
    margin-bottom: 1em;
  }
  .badge {
    font-weight: 600;
  }
  .badge.ok {
    color: #3ddc84;
  }
  .badge.off {
    color: #888;
  }
  .badge.wait {
    color: #e0b341;
  }
  .error {
    margin-top: 0.5em;
    color: #ff6b6b;
    font-size: 0.85em;
    max-width: 520px;
    word-break: break-word;
  }
  .info {
    text-align: left;
    background: #161616;
    border: 1px solid #333;
    border-radius: 8px;
    padding: 1em;
    max-width: 560px;
    max-height: 300px;
    overflow: auto;
    font-size: 0.8em;
    white-space: pre-wrap;
    word-break: break-word;
  }
</style>
