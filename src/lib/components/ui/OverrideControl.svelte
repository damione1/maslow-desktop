<script lang="ts">
  import IconButton from "./IconButton.svelte";

  let {
    label = "Feed",
    value = 100,
    disabled = false,
    onUp = undefined,
    onDown = undefined,
    onReset = undefined,
  }: {
    label?: string;
    value?: number;
    disabled?: boolean;
    onUp?: () => void;
    onDown?: () => void;
    onReset?: () => void;
  } = $props();
</script>

<div class="override">
  <span class="label">{label}</span>
  <IconButton title="{label} −10%" {disabled} onclick={() => onDown?.()}>−</IconButton>
  <button class="pct" {disabled} title="Reset {label} to 100%" onclick={() => onReset?.()}>
    {value}%
  </button>
  <IconButton title="{label} +10%" {disabled} onclick={() => onUp?.()}>+</IconButton>
</div>

<style>
  .override {
    display: grid;
    grid-template-columns: 1fr auto auto auto;
    align-items: center;
    gap: var(--gap-sm);
  }
  .label {
    color: var(--text-dim);
    font-size: 0.9em;
  }
  .pct {
    min-width: 4.5em;
    min-height: var(--tap);
    border: 1px solid var(--border-2);
    border-radius: var(--radius);
    background: var(--surface-2);
    color: var(--text);
    font-family: var(--mono);
    font-variant-numeric: tabular-nums;
    font-weight: 600;
    cursor: pointer;
  }
  .pct:hover:not(:disabled) {
    background: var(--surface-3);
  }
  .pct:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
</style>
