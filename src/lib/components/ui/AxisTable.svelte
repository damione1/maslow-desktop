<script lang="ts">
  import { machineStatus } from "$lib/stores/machine";
  import IconButton from "./IconButton.svelte";

  let {
    canZero = false,
    canGoZero = false,
    onZero = undefined,
    onGoZero = undefined,
    compact = false,
  }: {
    canZero?: boolean;
    canGoZero?: boolean;
    onZero?: (index: number, axis: string) => void;
    onGoZero?: (index: number, axis: string) => void;
    compact?: boolean;
  } = $props();

  const AXES = ["X", "Y", "Z"];
  const s = $derived($machineStatus);

  function fmt(n: number | undefined): string {
    return n === undefined ? "—" : n.toFixed(3);
  }
  function wpos(i: number): number | undefined {
    if (!s) return undefined;
    return (s.wpos.length ? s.wpos : s.mpos)[i];
  }
</script>

<div class="axis-table" class:compact>
  <div class="head">
    <span>Axis</span>
    <span>Work [G54]</span>
    <span>Machine</span>
    {#if !compact}<span class="acts-head"></span>{/if}
  </div>
  {#each AXES as axis, i}
    <div class="row">
      <span class="axis">{axis}</span>
      <span class="val work">{fmt(wpos(i))}</span>
      <span class="val machine">{fmt(s?.mpos[i])}</span>
      {#if !compact && (onZero || onGoZero)}
        <span class="acts">
          {#if onZero}
            <IconButton
              variant="datum"
              title="Set {axis} home — define the work {axis} for the current position"
              disabled={!canZero}
              onclick={() => onZero?.(i, axis)}>⌖</IconButton
            >
          {/if}
          {#if onGoZero}
            <IconButton
              variant="action"
              title="Go to {axis} home — the machine will move to work {axis} 0"
              disabled={!canGoZero}
              onclick={() => onGoZero?.(i, axis)}>⌂</IconButton
            >
          {/if}
        </span>
      {/if}
    </div>
  {/each}
</div>

<style>
  .axis-table {
    display: flex;
    flex-direction: column;
    gap: 2px;
    background: var(--border);
    border: 1px solid var(--border-2);
    border-radius: var(--radius);
    overflow: hidden;
  }
  .head,
  .row {
    display: grid;
    grid-template-columns: 56px 1fr 1fr auto;
    align-items: center;
    gap: var(--gap-sm);
    background: var(--surface);
    padding: 0.35em 0.6em;
  }
  .axis-table.compact .head,
  .axis-table.compact .row {
    grid-template-columns: 48px 1fr 1fr;
  }
  .head {
    background: var(--bar);
    font-size: 0.78em;
    color: var(--text-dim);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .axis {
    font-weight: 700;
    color: var(--text-dim);
  }
  .val {
    font-family: var(--mono);
    font-variant-numeric: tabular-nums;
    text-align: right;
  }
  .work {
    font-size: 1.4em;
    font-weight: 600;
  }
  .axis-table.compact .work {
    font-size: 1.1em;
  }
  .machine {
    font-size: 0.95em;
    color: var(--text-mute);
  }
  .acts {
    display: inline-flex;
    gap: 4px;
    justify-content: flex-end;
  }
</style>
