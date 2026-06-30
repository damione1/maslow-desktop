<script lang="ts">
  let {
    value = 0,
    max = 100,
    variant = "active",
    label = undefined,
  }: {
    value?: number;
    max?: number;
    variant?: "action" | "active" | "warn";
    label?: string;
  } = $props();

  const pct = $derived(max > 0 ? Math.min(100, Math.max(0, (value / max) * 100)) : 0);
</script>

<div class="bar {variant}" role="progressbar" aria-valuenow={value} aria-valuemax={max}>
  <div class="fill" style:width="{pct}%"></div>
  <span class="label">{label ?? `${Math.round(pct)}%`}</span>
</div>

<style>
  .bar {
    position: relative;
    width: 100%;
    height: calc(var(--tap) * 0.5);
    min-height: 22px;
    background: var(--inset);
    border: 1px solid var(--border-2);
    border-radius: var(--radius);
    overflow: hidden;
  }
  .fill {
    position: absolute;
    inset: 0 auto 0 0;
    background: var(--active);
    transition: width 0.2s ease;
  }
  .bar.action .fill {
    background: var(--action);
  }
  .bar.warn .fill {
    background: var(--warn);
  }
  .label {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 0.82em;
    font-weight: 600;
    color: var(--text);
    text-shadow: 0 1px 2px rgba(0, 0, 0, 0.6);
  }
</style>
