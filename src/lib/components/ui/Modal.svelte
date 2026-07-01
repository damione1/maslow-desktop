<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    title,
    onclose,
    children,
  }: {
    title: string;
    onclose: () => void;
    children: Snippet;
  } = $props();

  function onkeydown(e: KeyboardEvent) {
    if (e.key === "Escape") onclose();
  }
</script>

<svelte:window {onkeydown} />

<button class="backdrop" onclick={onclose} aria-label="Close dialog"></button>
<div class="card" role="dialog" aria-modal="true" aria-label={title}>
  <header>
    <span>{title}</span>
    <button class="x" onclick={onclose} aria-label="Close">✕</button>
  </header>
  <div class="body">
    {@render children()}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.55);
    border: none;
    padding: 0;
    margin: 0;
    cursor: default;
    z-index: 50;
  }
  .card {
    position: fixed;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    z-index: 51;
    width: min(440px, calc(100vw - 2em));
    max-height: calc(100vh - 2em);
    overflow: auto;
    background: var(--surface);
    border: 1px solid var(--border-3);
    border-radius: var(--radius-lg);
    box-shadow: 0 12px 40px rgba(0, 0, 0, 0.5);
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.7em 0.9em;
    border-bottom: 1px solid var(--border);
    font-weight: 600;
  }
  .x {
    background: transparent;
    border: none;
    color: var(--text-dim);
    font-size: 1em;
    cursor: pointer;
    padding: 0.2em 0.4em;
    border-radius: var(--radius);
  }
  .x:hover {
    background: var(--border);
    color: #fff;
  }
  .body {
    padding: 0.9em;
  }
</style>
