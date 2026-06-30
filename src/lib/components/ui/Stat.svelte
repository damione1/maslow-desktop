<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    label,
    value = undefined,
    children = undefined,
  }: {
    label: string;
    value?: string | number;
    children?: Snippet;
  } = $props();
</script>

<div class="stat">
  <span class="label">{label}</span>
  <span class="value">
    {#if children}{@render children()}{:else}{value}{/if}
  </span>
</div>

<style>
  .stat {
    display: grid;
    grid-template-columns: 9.5em 1fr;
    align-items: center;
    gap: var(--gap-sm);
    padding: 0.3em 0;
    border-bottom: 1px solid var(--border);
  }
  .stat:last-child {
    border-bottom: none;
  }
  .label {
    color: var(--text-dim);
    font-size: 0.9em;
  }
  .value {
    font-family: var(--mono);
    font-variant-numeric: tabular-nums;
    font-size: 0.95em;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
