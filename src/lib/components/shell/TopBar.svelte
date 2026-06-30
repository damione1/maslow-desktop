<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { connection, fwVersion, connectWs, disconnectWs } from "$lib/stores/connection";
  import { firmwareNotice } from "$lib/stores/firmware";
  import { wsState } from "$lib/stores/machine";
  import { activeTab, type Tab } from "$lib/stores/ui";

  const connected = $derived($wsState === "connected");

  const TABS: { id: Tab; label: string; glyph: string }[] = [
    { id: "main", label: "Main", glyph: "⌂" },
    { id: "run", label: "Run", glyph: "▶" },
    { id: "calibrate", label: "Calibrate", glyph: "◎" },
    { id: "files", label: "Files", glyph: "▤" },
    { id: "config", label: "Config", glyph: "⚙" },
  ];

  let host = $state($connection.host);
  let connOpen = $state(false);

  async function connect() {
    await connectWs(host);
  }
  async function disconnect() {
    await disconnectWs();
  }

  // 0x18 = Ctrl-X soft reset: the universal mid-cut kill. Always reachable from
  // the chrome, never gated behind a tab.
  function abort() {
    invoke("send_realtime", { byte: 0x18 });
  }
</script>

<div class="topbar">
  <div class="tabs" role="tablist">
    {#each TABS as t}
      <button
        role="tab"
        aria-selected={$activeTab === t.id}
        class="tab"
        class:active={$activeTab === t.id}
        onclick={() => activeTab.set(t.id)}
      >
        <span class="glyph">{t.glyph}</span>
        <span class="tab-label">{t.label}</span>
      </button>
    {/each}
  </div>

  <div class="right">
    {#if $fwVersion}
      <span class="fw" class:untested={$firmwareNotice} title={$firmwareNotice ?? "Maslow firmware version"}
        >FW {$fwVersion}</span
      >
    {/if}

    <div class="conn-wrap">
      <button
        class="conn-chip"
        class:ok={connected}
        onclick={() => (connOpen = !connOpen)}
        title={connected ? `Connected to ${$connection.host}` : "Not connected"}
      >
        <span class="dot"></span>
        <span class="conn-label">{connected ? $connection.host : "Disconnected"}</span>
      </button>

      {#if connOpen}
        <div class="conn-pop">
          <label for="host-input">Host</label>
          <input
            id="host-input"
            class="host"
            placeholder="maslow.local or IP"
            bind:value={host}
            autocomplete="off"
            spellcheck="false"
          />
          {#if connected}
            <button class="pop-btn ghost" onclick={disconnect}>Disconnect</button>
          {:else}
            <button class="pop-btn" onclick={connect}>Connect</button>
            {#if $connection.error}
              <span class="conn-err">{$connection.error}</span>
            {/if}
          {/if}
        </div>
      {/if}
    </div>

    <button
      class="abort"
      onclick={abort}
      disabled={!connected}
      title="Emergency stop — halts all motion immediately (realtime soft reset, Ctrl-X / 0x18). Recoverable: unlock or home afterward."
    >
      ⛔ <span class="abort-label">ABORT</span>
    </button>
  </div>
</div>

<style>
  .topbar {
    display: flex;
    align-items: stretch;
    gap: var(--gap-sm);
    padding: 6px 10px;
    background: var(--bar);
    border-bottom: 1px solid var(--border-2);
  }
  .tabs {
    display: flex;
    gap: 4px;
    align-items: stretch;
  }
  .tab {
    display: inline-flex;
    align-items: center;
    gap: 0.45em;
    min-height: var(--tap);
    padding: 0 1em;
    border: 1px solid transparent;
    border-radius: var(--radius);
    background: var(--surface-2);
    color: var(--text-dim);
    font-family: var(--font);
    font-weight: 600;
    font-size: 1em;
    cursor: pointer;
    transition:
      background 0.12s ease,
      color 0.12s ease;
  }
  .tab:hover {
    background: var(--surface-3);
    color: var(--text);
  }
  .tab.active {
    background: var(--active);
    color: var(--active-text);
  }
  .glyph {
    font-size: 1.1em;
    line-height: 1;
  }

  .right {
    display: flex;
    align-items: center;
    gap: var(--gap-sm);
    margin-left: auto;
  }
  .fw {
    font-size: 0.78em;
    color: #9bb4d8;
    background: #20304d;
    padding: 0.25em 0.55em;
    border-radius: var(--radius);
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }
  .fw.untested {
    color: var(--warn);
    background: #3a2a14;
    border: 1px solid #6b4a1f;
  }

  .conn-wrap {
    position: relative;
  }
  .conn-chip {
    display: inline-flex;
    align-items: center;
    gap: 0.5em;
    min-height: var(--tap);
    padding: 0 0.9em;
    border: 1px solid var(--border-2);
    border-radius: var(--radius);
    background: var(--surface-2);
    color: var(--text-dim);
    cursor: pointer;
    font-size: 0.9em;
  }
  .conn-chip .dot {
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: var(--text-mute);
  }
  .conn-chip.ok {
    color: var(--text);
  }
  .conn-chip.ok .dot {
    background: #3ddc84;
  }
  .conn-pop {
    position: absolute;
    top: calc(100% + 6px);
    right: 0;
    z-index: 60;
    display: flex;
    flex-direction: column;
    gap: var(--gap-sm);
    width: 260px;
    padding: 0.9em;
    background: var(--surface);
    border: 1px solid var(--border-3);
    border-radius: var(--radius-lg);
    box-shadow: 0 12px 40px rgba(0, 0, 0, 0.5);
  }
  .conn-pop label {
    font-size: 0.8em;
    color: var(--text-dim);
  }
  .host {
    padding: 0.55em 0.8em;
    border-radius: var(--radius);
    border: 1px solid var(--border-3);
    background: var(--surface-3);
    color: #fff;
    font-size: 0.95em;
  }
  .pop-btn {
    min-height: var(--tap);
    border-radius: var(--radius);
    border: 1px solid var(--action);
    background: var(--action);
    color: #fff;
    cursor: pointer;
    font-weight: 600;
  }
  .pop-btn.ghost {
    background: transparent;
    border-color: var(--border-3);
  }
  .conn-err {
    font-size: 0.78em;
    color: #ff8a8a;
  }

  .abort {
    display: inline-flex;
    align-items: center;
    gap: 0.4em;
    min-height: var(--tap);
    padding: 0 1.1em;
    border: 1px solid var(--danger);
    border-radius: var(--radius);
    background: var(--danger);
    color: #fff;
    font-weight: 800;
    letter-spacing: 0.03em;
    cursor: pointer;
    white-space: nowrap;
  }
  .abort:hover:not(:disabled) {
    background: var(--danger-hover);
  }
  .abort:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  /* Narrow viewports: collapse tab + ABORT text to glyphs only. */
  @media (max-width: 720px) {
    .tab-label,
    .conn-label {
      display: none;
    }
    .tab {
      padding: 0 0.7em;
    }
  }
  @media (max-width: 480px) {
    .abort-label {
      display: none;
    }
    .fw {
      display: none;
    }
  }
</style>
