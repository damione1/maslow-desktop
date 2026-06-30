<script lang="ts">
  import { connection } from "$lib/stores/connection";
  import { firmwareNotice } from "$lib/stores/firmware";
  import { machineStatus, wsState, stateClass } from "$lib/stores/machine";

  const connected = $derived($wsState === "connected");
  const s = $derived($machineStatus);
  const feedOv = $derived(s?.ov?.[0]);
</script>

<div class="strip">
  <span class="item state">
    <span class="dot {s ? stateClass(s.state) : 'other'}"></span>
    {s ? s.state : "—"}
  </span>

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

  {#if $firmwareNotice}
    <span class="item notice" title={$firmwareNotice}>⚠ Firmware untested</span>
  {/if}
</div>

<style>
  .strip {
    display: flex;
    align-items: center;
    gap: var(--gap-lg);
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
  .state {
    font-weight: 600;
    color: var(--text);
  }
  .dot {
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: var(--state-other);
  }
  .dot.idle {
    background: var(--state-idle);
  }
  .dot.run {
    background: var(--state-run);
  }
  .dot.hold {
    background: var(--state-hold);
  }
  .dot.alarm {
    background: var(--state-alarm);
  }
  .notice {
    color: var(--warn);
    margin-left: auto;
  }
</style>
