<script lang="ts">
  import { toolpath, toolpathPath, jobProgress } from "$lib/stores/job";

  // The app parses G-code into a toolpath (segments with source line numbers); it
  // does not keep the raw source text. This list renders those parsed moves and
  // highlights progress from the streamer's acked line, so it tracks the cut
  // without fabricating source lines.
  //
  // A loaded file can have well over 100k segments, so this is a windowed
  // virtual list: only the rows that fit in the visible area (plus a small
  // overscan) are ever in the DOM. Row height is measured once from a hidden
  // probe row so the spacer math lines up with the real row styling.
  const tp = $derived($toolpath);

  const progressLine = $derived.by(() => {
    const j = $jobProgress;
    if (!j || ($toolpathPath && j.path !== $toolpathPath)) return null;
    if (j.state !== "running" && j.state !== "paused" && j.state !== "interrupted") return null;
    return j.acked;
  });

  const OVERSCAN = 20;
  const FOLLOW_INTERVAL_MS = 200;
  const MANUAL_SCROLL_QUIET_MS = 5000;
  const FALLBACK_ROW_HEIGHT = 20;

  let listEl: HTMLDivElement | undefined = $state();
  let probeEl: HTMLDivElement | undefined = $state();

  let rowHeight = $state(FALLBACK_ROW_HEIGHT);
  let scrollTop = $state(0);
  let viewportHeight = $state(0);

  let rafPending = false;
  // Set right before we programmatically assign scrollTop, cleared shortly
  // after, so the scroll handler can tell "we scrolled it" from "the user
  // scrolled it" and only treat the latter as manual.
  let programmaticScroll = false;
  let lastManualScrollAt = 0;
  let lastFollowAt = 0;

  const total = $derived(tp?.segments.length ?? 0);

  const startIndex = $derived(Math.max(0, Math.floor(scrollTop / rowHeight) - OVERSCAN));
  const endIndex = $derived(
    Math.min(total, Math.ceil((scrollTop + viewportHeight) / rowHeight) + OVERSCAN),
  );

  const visible = $derived(tp ? tp.segments.slice(startIndex, endIndex) : []);
  const topPad = $derived(startIndex * rowHeight);
  const bottomPad = $derived(Math.max(0, (total - endIndex) * rowHeight));

  // Measure the real row height once the probe row is in the DOM.
  $effect(() => {
    if (!probeEl) return;
    const h = probeEl.getBoundingClientRect().height;
    if (h > 0) rowHeight = h;
  });

  // Track the viewport height as the panel resizes.
  $effect(() => {
    if (!listEl) return;
    viewportHeight = listEl.clientHeight;
    const ro = new ResizeObserver(() => {
      if (listEl) viewportHeight = listEl.clientHeight;
    });
    ro.observe(listEl);
    return () => ro.disconnect();
  });

  // A new file was loaded: the old scroll position (and manual-scroll quiet
  // period) no longer means anything.
  $effect(() => {
    void tp;
    if (listEl) listEl.scrollTop = 0;
    scrollTop = 0;
    lastManualScrollAt = 0;
    lastFollowAt = 0;
  });

  function onScroll() {
    if (!listEl) return;
    if (!programmaticScroll) {
      lastManualScrollAt = Date.now();
    }
    if (rafPending) return;
    rafPending = true;
    requestAnimationFrame(() => {
      rafPending = false;
      if (!listEl) return;
      scrollTop = listEl.scrollTop;
      viewportHeight = listEl.clientHeight;
    });
  }

  // Keep the current move in view as the cut advances, but stay out of the
  // way while the operator is manually reading the list.
  $effect(() => {
    const line = progressLine;
    if (line == null || !listEl || !tp || tp.segments.length === 0) return;
    const now = Date.now();
    if (now - lastFollowAt < FOLLOW_INTERVAL_MS) return;
    if (now - lastManualScrollAt < MANUAL_SCROLL_QUIET_MS) return;
    lastFollowAt = now;

    const segs = tp.segments;
    let idx = segs.findIndex((s) => s.line >= line);
    if (idx === -1) idx = segs.length - 1;

    const targetTop = Math.max(0, idx * rowHeight - viewportHeight / 2 + rowHeight / 2);
    programmaticScroll = true;
    listEl.scrollTop = targetTop;
    scrollTop = targetTop;
    requestAnimationFrame(() => {
      programmaticScroll = false;
    });
  });

  function move(s: { rapid: boolean; x1: number; y1: number }): string {
    return `${s.rapid ? "G0" : "G1"} X${s.x1.toFixed(2)} Y${s.y1.toFixed(2)}`;
  }
</script>

<div class="list" bind:this={listEl} onscroll={onScroll}>
  {#if !tp || tp.segments.length === 0}
    <div class="empty">No toolpath loaded.</div>
  {:else}
    <div class="row probe" bind:this={probeEl} aria-hidden="true">
      <span class="ln">0</span>
      <span class="code">G0 X0.00 Y0.00</span>
    </div>
    <div style={`height:${topPad}px`}></div>
    {#each visible as s (s.line)}
      {@const done = progressLine != null && s.line < progressLine}
      {@const current = progressLine != null && s.line === progressLine}
      <div class="row" class:done class:current>
        <span class="ln">{s.line}</span>
        <span class="code" class:rapid={s.rapid}>{move(s)}</span>
      </div>
    {/each}
    <div style={`height:${bottomPad}px`}></div>
  {/if}
</div>

<style>
  .list {
    position: relative;
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
  .row.probe {
    position: absolute;
    top: 0;
    left: 0;
    width: 100%;
    visibility: hidden;
    pointer-events: none;
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
