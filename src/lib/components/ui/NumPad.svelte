<script lang="ts">
  let {
    value = $bindable(""),
    allowNegative = true,
    onenter = undefined,
  }: {
    value: string;
    allowNegative?: boolean;
    onenter?: (v: string) => void;
  } = $props();

  const KEYS = ["7", "8", "9", "4", "5", "6", "1", "2", "3"];

  function press(k: string) {
    if (k === "." && value.includes(".")) return;
    value = value + k;
  }
  function backspace() {
    value = value.slice(0, -1);
  }
  function toggleSign() {
    if (!allowNegative) return;
    value = value.startsWith("-") ? value.slice(1) : "-" + value;
  }
</script>

<div class="numpad">
  <div class="display">{value || "0"}</div>
  <div class="grid">
    {#each KEYS as k}
      <button class="key" onclick={() => press(k)}>{k}</button>
    {/each}
    <button class="key" onclick={toggleSign} disabled={!allowNegative}>±</button>
    <button class="key" onclick={() => press("0")}>0</button>
    <button class="key" onclick={() => press(".")}>.</button>
    <button class="key wide back" onclick={backspace} aria-label="Backspace">⌫</button>
    <button class="key wide ok" onclick={() => onenter?.(value)} aria-label="Confirm">✓</button>
  </div>
</div>

<style>
  .numpad {
    display: flex;
    flex-direction: column;
    gap: var(--gap-sm);
  }
  .display {
    min-height: var(--tap);
    display: flex;
    align-items: center;
    justify-content: flex-end;
    padding: 0 0.7em;
    background: var(--inset);
    border: 1px solid var(--border-2);
    border-radius: var(--radius);
    font-family: var(--mono);
    font-size: 1.4em;
    font-variant-numeric: tabular-nums;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: var(--gap-sm);
  }
  .key {
    min-height: var(--tap);
    border: 1px solid var(--border-2);
    border-radius: var(--radius);
    background: var(--surface-2);
    color: var(--text);
    font-family: var(--font);
    font-size: 1.2em;
    font-weight: 600;
    cursor: pointer;
  }
  .key:hover:not(:disabled) {
    background: var(--surface-3);
  }
  .key:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
  .key.wide {
    grid-column: span 1;
  }
  .key.ok {
    background: var(--active);
    border-color: transparent;
    grid-column: span 2;
  }
  .key.back {
    grid-column: span 1;
  }
</style>
