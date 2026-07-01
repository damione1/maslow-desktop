<script lang="ts">
  import type { Snippet } from "svelte";

  type Variant = "action" | "datum" | "active" | "danger" | "ghost";
  type Size = "sm" | "md" | "lg";

  let {
    variant = "action",
    size = "md",
    disabled = false,
    active = false,
    title = undefined,
    type = "button",
    onclick = undefined,
    children,
  }: {
    variant?: Variant;
    size?: Size;
    disabled?: boolean;
    active?: boolean;
    title?: string;
    type?: "button" | "submit";
    onclick?: (e: MouseEvent) => void;
    children: Snippet;
  } = $props();
</script>

<button
  {type}
  {title}
  {disabled}
  class="btn {variant} {size}"
  class:active
  onclick={(e) => onclick?.(e)}
>
  {@render children()}
</button>

<style>
  .btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 0.5em;
    min-height: var(--tap);
    padding: 0 1.1em;
    border: 1px solid transparent;
    border-radius: var(--radius);
    font-family: var(--font);
    font-weight: 600;
    font-size: 1em;
    line-height: 1.1;
    color: var(--action-text);
    background: var(--action);
    cursor: pointer;
    transition:
      background 0.12s ease,
      opacity 0.12s ease,
      filter 0.12s ease;
    -webkit-tap-highlight-color: transparent;
    user-select: none;
  }
  .btn.sm {
    min-height: calc(var(--tap) * 0.72);
    padding: 0 0.8em;
    font-size: 0.9em;
  }
  .btn.lg {
    min-height: var(--tap-lg);
    padding: 0 1.4em;
    font-size: 1.15em;
    border-radius: var(--radius-lg);
  }

  .btn.action {
    background: var(--action);
    color: var(--action-text);
  }
  .btn.action:hover:not(:disabled) {
    background: var(--action-hover);
  }
  .btn.datum {
    background: var(--datum);
    color: var(--datum-text);
  }
  .btn.datum:hover:not(:disabled) {
    background: var(--datum-hover);
  }
  .btn.active {
    background: var(--active);
    color: var(--active-text);
  }
  .btn.active:hover:not(:disabled) {
    background: var(--active-hover);
  }
  .btn.danger {
    background: var(--danger);
    color: var(--danger-text);
  }
  .btn.danger:hover:not(:disabled) {
    background: var(--danger-hover);
  }
  .btn.ghost {
    background: var(--ghost-bg);
    color: var(--text);
    border-color: var(--border-2);
  }
  .btn.ghost:hover:not(:disabled) {
    background: var(--ghost-hover);
  }

  /* Selected/toggled state overrides variant color with the green "active" hue. */
  .btn.active.active,
  .btn.action.active,
  .btn.ghost.active {
    background: var(--active);
    color: var(--active-text);
    border-color: transparent;
  }

  .btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
  .btn:active:not(:disabled) {
    filter: brightness(0.9);
  }
</style>
