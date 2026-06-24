<script lang="ts">
  import Console from "./Console.svelte";
  import {
    consoleCollapsed,
    consoleHeight,
    CONSOLE_MIN_HEIGHT,
  } from "$lib/stores/ui";

  let dragging = $state(false);
  let startY = 0;
  let startH = 0;

  function onPointerDown(e: PointerEvent) {
    dragging = true;
    startY = e.clientY;
    startH = $consoleHeight;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }

  function onPointerMove(e: PointerEvent) {
    if (!dragging) return;
    const max = window.innerHeight * 0.5;
    // Drag up (smaller clientY) grows the dock.
    const next = Math.min(max, Math.max(CONSOLE_MIN_HEIGHT, startH + (startY - e.clientY)));
    consoleHeight.set(next);
  }

  function onPointerUp(e: PointerEvent) {
    if (!dragging) return;
    dragging = false;
    try {
      (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
    } catch {
      /* pointer already released */
    }
  }

  const toggle = () => consoleCollapsed.update((v) => !v);
</script>

<section
  class="dock"
  class:collapsed={$consoleCollapsed}
  style:height={$consoleCollapsed ? null : `${$consoleHeight}px`}
>
  <div
    class="grip"
    class:idle={$consoleCollapsed}
    role="separator"
    aria-orientation="horizontal"
    aria-label="Resize console"
    onpointerdown={$consoleCollapsed ? undefined : onPointerDown}
    onpointermove={onPointerMove}
    onpointerup={onPointerUp}
  >
    <button
      class="chevron"
      onclick={toggle}
      aria-expanded={!$consoleCollapsed}
      title={$consoleCollapsed ? "Expand console" : "Collapse console"}
    >
      {$consoleCollapsed ? "▲" : "▼"}
      {#if $consoleCollapsed}<span class="label">Console</span>{/if}
    </button>
  </div>

  {#if !$consoleCollapsed}
    <div class="body">
      <Console />
    </div>
  {/if}
</section>

<style>
  .dock {
    display: flex;
    flex-direction: column;
    background: #0e0e0e;
    border-top: 1px solid #333;
  }
  .grip {
    display: flex;
    align-items: center;
    height: 16px;
    cursor: row-resize;
    background: #161616;
    border-bottom: 1px solid #222;
    flex: 0 0 auto;
    touch-action: none;
  }
  .grip.idle {
    cursor: default;
    height: 28px;
  }
  .chevron {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 100%;
    padding: 0 0.9em;
    border: none;
    background: transparent;
    color: #999;
    cursor: pointer;
    font-size: 0.8em;
  }
  .chevron:hover {
    color: #ddd;
  }
  .label {
    font-weight: 600;
  }
  .body {
    flex: 1;
    min-height: 0;
    display: flex;
    /* Breathing room so the console card isn't flush against the dock edges. */
    padding: 8px 10px 10px;
  }
</style>
