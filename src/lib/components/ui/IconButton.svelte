<script lang="ts">
  import type { Snippet } from "svelte";

  type Variant = "action" | "datum" | "active" | "danger" | "ghost";

  let {
    variant = "ghost",
    disabled = false,
    title = undefined,
    onclick = undefined,
    children,
  }: {
    variant?: Variant;
    disabled?: boolean;
    title?: string;
    onclick?: (e: MouseEvent) => void;
    children: Snippet;
  } = $props();
</script>

<button {title} {disabled} class="icon-btn {variant}" onclick={(e) => onclick?.(e)}>
  {@render children()}
</button>

<style>
  .icon-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: var(--tap);
    height: var(--tap);
    min-width: var(--tap);
    border: 1px solid transparent;
    border-radius: var(--radius);
    color: var(--action-text);
    background: var(--action);
    cursor: pointer;
    font-size: 1em;
    transition: background 0.12s ease;
    -webkit-tap-highlight-color: transparent;
  }
  .icon-btn.action {
    background: var(--action);
  }
  .icon-btn.action:hover:not(:disabled) {
    background: var(--action-hover);
  }
  .icon-btn.datum {
    background: var(--datum);
    color: var(--datum-text);
  }
  .icon-btn.datum:hover:not(:disabled) {
    background: var(--datum-hover);
  }
  .icon-btn.active {
    background: var(--active);
  }
  .icon-btn.danger {
    background: var(--danger);
  }
  .icon-btn.ghost {
    background: var(--ghost-bg);
    color: var(--text);
    border-color: var(--border-2);
  }
  .icon-btn.ghost:hover:not(:disabled) {
    background: var(--ghost-hover);
  }
  .icon-btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
  .icon-btn:active:not(:disabled) {
    filter: brightness(0.9);
  }
</style>
