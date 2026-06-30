<script lang="ts">
  import { toolpath, toolpathPath, jobProgress } from "$lib/stores/job";

  // The app parses G-code into a toolpath (segments with source line numbers); it
  // does not keep the raw source text. This list renders those parsed moves and
  // highlights progress from the streamer's acked line, so it tracks the cut
  // without fabricating source lines.
  const tp = $derived($toolpath);

  const progressLine = $derived.by(() => {
    const j = $jobProgress;
    if (!j || ($toolpathPath && j.path !== $toolpathPath)) return null;
    if (j.state !== "running" && j.state !== "paused" && j.state !== "interrupted") return null;
    return j.acked;
  });

  let listEl: HTMLDivElement | undefined = $state();
  let currentEl: HTMLDivElement | undefined = $state();

  // Keep the current move in view as the cut advances.
  $effect(() => {
    void progressLine;
    if (currentEl && listEl) {
      currentEl.scrollIntoView({ block: "nearest" });
    }
  });

  function move(s: { rapid: boolean; x1: number; y1: number }): string {
    return `${s.rapid ? "G0" : "G1"} X${s.x1.toFixed(2)} Y${s.y1.toFixed(2)}`;
  }
</script>

<div class="list" bind:this={listEl}>
  {#if !tp || tp.segments.length === 0}
    <div class="empty">No toolpath loaded.</div>
  {:else}
    {#each tp.segments as s (s.line + "-" + s.x1 + "-" + s.y1)}
      {@const done = progressLine != null && s.line < progressLine}
      {@const current = progressLine != null && s.line === progressLine}
      <div class="row" class:done class:current bind:this={currentEl}>
        <span class="ln">{s.line}</span>
        <span class="code" class:rapid={s.rapid}>{move(s)}</span>
      </div>
    {/each}
  {/if}
</div>

<style>
  .list {
    height: 100%;
    overflow-y: auto;
    background: var(--inset);
    border: 1px solid var(--border-2);
    border-radius: var(--radius);
    font-family: var(--mono);
    font-size: 0.8em;
  }
  .empty {
    padding: 1em;
    color: var(--text-mute);
  }
  .row {
    display: grid;
    grid-template-columns: 3.5em 1fr;
    gap: 0.6em;
    padding: 0.15em 0.6em;
    border-bottom: 1px solid rgba(255, 255, 255, 0.03);
  }
  .ln {
    color: var(--text-mute);
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .code {
    color: var(--text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .code.rapid {
    color: var(--text-dim);
  }
  .row.done .code {
    color: #5a8a5f;
  }
  .row.current {
    background: var(--active);
  }
  .row.current .ln,
  .row.current .code {
    color: #fff;
  }
</style>
