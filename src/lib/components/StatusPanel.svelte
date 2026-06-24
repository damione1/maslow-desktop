<script lang="ts">
  import { machineStatus } from "$lib/stores/machine";

  const axes = ["X", "Y", "Z"];

  function fmt(n: number | undefined): string {
    return n === undefined ? "—" : n.toFixed(2);
  }

  function stateClass(state: string): string {
    switch (state) {
      case "Idle":
        return "idle";
      case "Run":
      case "Jog":
      case "Home":
        return "run";
      case "Hold":
      case "Door":
        return "hold";
      case "Alarm":
        return "alarm";
      default:
        return "other";
    }
  }
</script>

<section class="panel">
  {#if $machineStatus}
    {@const s = $machineStatus}
    <div class="state {stateClass(s.state)}">
      {s.state}{s.substate !== null ? `:${s.substate}` : ""}
    </div>

    <div class="axes">
      {#each axes as axis, i}
        <div class="axis">
          <span class="label">{axis}</span>
          <span class="wpos">{fmt((s.wpos.length ? s.wpos : s.mpos)[i])}</span>
          <span class="mpos">M {fmt(s.mpos[i])}</span>
        </div>
      {/each}
    </div>

    <div class="meta">
      <span>Feed {fmt(s.feed)}</span>
      <span>Spindle {fmt(s.spindle)}</span>
      {#if s.buffer_blocks !== null}
        <span>Buf {s.buffer_blocks}/{s.buffer_bytes}</span>
      {/if}
    </div>
  {:else}
    <div class="state other">Waiting for status…</div>
  {/if}
</section>

<style>
  .panel {
    background: #161616;
    border: 1px solid #333;
    border-radius: 10px;
    padding: 1em 1.2em;
  }
  .state {
    display: inline-block;
    font-weight: 700;
    font-size: 1.1em;
    padding: 0.2em 0.8em;
    border-radius: 6px;
    margin-bottom: 0.8em;
  }
  .state.idle {
    background: #2e7d32;
  }
  .state.run {
    background: #1565c0;
  }
  .state.hold {
    background: #b8860b;
  }
  .state.alarm {
    background: #c62828;
  }
  .state.other {
    background: #444;
  }
  .axes {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 10px;
  }
  .axis {
    display: flex;
    flex-direction: column;
    background: #1f1f1f;
    border-radius: 8px;
    padding: 0.6em;
    text-align: center;
  }
  .label {
    font-size: 0.8em;
    opacity: 0.6;
  }
  .wpos {
    font-size: 1.6em;
    font-variant-numeric: tabular-nums;
    font-weight: 600;
  }
  .mpos {
    font-size: 0.7em;
    opacity: 0.5;
  }
  .meta {
    margin-top: 0.8em;
    display: flex;
    gap: 1.2em;
    font-size: 0.85em;
    opacity: 0.8;
  }
</style>
