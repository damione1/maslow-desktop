<script lang="ts">
  import { onMount } from "svelte";
  import { apiSettings, refreshApiSettings, setApiEnabled, regenerateApiKey } from "$lib/stores/apiSettings";
  import { confirmDialog } from "$lib/stores/confirm";
  import Button from "$lib/components/ui/Button.svelte";
  import Modal from "$lib/components/ui/Modal.svelte";
  import Stat from "$lib/components/ui/Stat.svelte";

  onMount(() => {
    refreshApiSettings();
  });

  const settings = $derived($apiSettings);
  const enabled = $derived(settings?.enabled ?? false);
  const hasKey = $derived(settings?.has_key ?? false);
  const listening = $derived(settings?.listening ?? false);

  let toggling = $state(false);
  let regenerating = $state(false);
  let revealedKey = $state<string | null>(null);
  let copied = $state(false);

  async function toggleEnabled() {
    if (!settings) return;
    toggling = true;
    try {
      await setApiEnabled(!settings.enabled);
    } finally {
      toggling = false;
    }
  }

  async function regenerate() {
    const message = hasKey
      ? "Regenerate the API key? Any client using the current key will be rejected immediately."
      : "Generate an API key? This is required before the API can be enabled.";
    if (!(await confirmDialog(message, { danger: hasKey }))) return;
    regenerating = true;
    try {
      revealedKey = await regenerateApiKey();
      copied = false;
    } finally {
      regenerating = false;
    }
  }

  async function copyKey() {
    if (!revealedKey) return;
    await navigator.clipboard.writeText(revealedKey);
    copied = true;
  }

  function dismissKey() {
    revealedKey = null;
    copied = false;
  }
</script>

<div class="settings-tab">
  <section class="card">
    <h3>Machine API</h3>
    <p class="blurb">
      Exposes this machine's control API (an HTTP gateway and gRPC, with an MCP server planned)
      to any client on localhost that presents the current API key. Only turn this on if you
      intend to drive the machine from another local tool.
    </p>

    {#if !settings}
      <div class="hint">Loading…</div>
    {:else}
      <div class="row">
        <span class="row-label">API access</span>
        <Button
          variant={enabled ? "active" : "ghost"}
          active={enabled}
          disabled={toggling || (!enabled && !hasKey)}
          title={!hasKey && !enabled ? "Generate an API key before enabling" : undefined}
          onclick={toggleEnabled}
        >
          {enabled ? "Enabled" : "Disabled"}
        </Button>
      </div>
      {#if !hasKey}
        <div class="warn">No API key generated yet. Generate one below before enabling access.</div>
      {/if}

      <div class="row">
        <span class="row-label">Status</span>
        <span class="status-pill" class:on={listening}>
          {#if listening}
            Listening on 127.0.0.1:{settings.port_http} (HTTP) / 127.0.0.1:{settings.port_grpc} (gRPC)
          {:else}
            Stopped
          {/if}
        </span>
      </div>

      <div class="stats">
        <Stat label="HTTP port" value={settings.port_http} />
        <Stat label="gRPC port" value={settings.port_grpc} />
        <Stat label="API key" value={hasKey ? "Set (hidden)" : "No API key generated yet"} />
      </div>

      <Button variant="datum" disabled={regenerating} onclick={regenerate}>
        {hasKey ? "Regenerate key" : "Generate key"}
      </Button>
    {/if}
  </section>
</div>

{#if revealedKey}
  <Modal title="New API key" onclose={dismissKey}>
    <p class="key-warning">
      Copy this key now: it will not be shown again. Only its hash is stored, so if you lose it
      you will need to generate a new one.
    </p>
    <code class="key-value">{revealedKey}</code>
    <div class="key-actions">
      <Button variant="ghost" onclick={copyKey}>{copied ? "Copied" : "Copy"}</Button>
      <Button variant="action" onclick={dismissKey}>Done</Button>
    </div>
  </Modal>
{/if}

<style>
  .settings-tab {
    padding: var(--gap-lg);
  }
  .card {
    max-width: 640px;
    background: var(--surface);
    border: 1px solid var(--border-2);
    border-radius: var(--radius-lg);
    padding: var(--gap-lg);
    display: flex;
    flex-direction: column;
    gap: var(--gap);
  }
  h3 {
    margin: 0;
    font-size: 0.9em;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-dim);
  }
  .blurb {
    margin: 0;
    font-size: 0.85em;
    line-height: 1.4;
    color: var(--text-dim);
  }
  .hint {
    font-size: 0.85em;
    color: var(--text-mute);
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--gap);
  }
  .row-label {
    font-size: 0.9em;
    color: var(--text-dim);
  }
  .status-pill {
    font-size: 0.85em;
    font-family: var(--mono);
    padding: 0.25em 0.7em;
    border-radius: var(--radius);
    background: var(--surface-3);
    color: var(--text-dim);
  }
  .status-pill.on {
    color: #3ddc84;
  }
  .stats {
    display: flex;
    flex-direction: column;
  }
  .warn {
    font-size: 0.8em;
    line-height: 1.35;
    color: var(--warn);
    background: #2a2008;
    border: 1px solid #6b4a1f;
    border-radius: var(--radius);
    padding: 0.5em 0.7em;
  }
  .key-warning {
    margin: 0 0 1em;
    font-size: 0.9em;
    line-height: 1.4;
    color: var(--warn);
  }
  .key-value {
    display: block;
    word-break: break-all;
    background: var(--surface-3);
    border: 1px solid var(--border-2);
    border-radius: var(--radius);
    padding: 0.7em 0.9em;
    font-family: var(--mono);
    font-size: 0.95em;
    margin-bottom: 1em;
  }
  .key-actions {
    display: flex;
    justify-content: flex-end;
    gap: 0.6em;
  }
</style>
