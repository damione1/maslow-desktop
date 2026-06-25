<script lang="ts">
  import { waypoints, calComplete, maslowState } from "$lib/stores/maslow";

  let canvas: HTMLCanvasElement | undefined = $state();

  const pts = $derived($waypoints);
  // The map only has anything to show while calibration is collecting points
  // (or after it has). At idle it's a useless black void, so collapse it.
  const active = $derived(pts.length > 0 || ($maslowState?.busy ?? false));

  function draw() {
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    const w = canvas.width;
    const h = canvas.height;
    ctx.clearRect(0, 0, w, h);

    if (pts.length === 0) {
      ctx.fillStyle = "#555";
      ctx.font = "12px sans-serif";
      ctx.fillText("no waypoints yet", 10, 20);
      return;
    }

    // Fit the point cloud into the canvas with padding, Y up.
    const pad = 18;
    const xs = pts.map((p) => p.x);
    const ys = pts.map((p) => p.y);
    let minX = Math.min(...xs),
      maxX = Math.max(...xs);
    let minY = Math.min(...ys),
      maxY = Math.max(...ys);
    const spanX = maxX - minX || 1;
    const spanY = maxY - minY || 1;
    const scale = Math.min((w - 2 * pad) / spanX, (h - 2 * pad) / spanY);
    const ox = (w - spanX * scale) / 2;
    const oy = (h - spanY * scale) / 2;
    const sx = (x: number) => ox + (x - minX) * scale;
    const sy = (y: number) => h - (oy + (y - minY) * scale); // flip Y

    // Path between waypoints in order.
    ctx.strokeStyle = "#2f4f7f";
    ctx.lineWidth = 1;
    ctx.beginPath();
    pts.forEach((p, i) => {
      const X = sx(p.x);
      const Y = sy(p.y);
      i === 0 ? ctx.moveTo(X, Y) : ctx.lineTo(X, Y);
    });
    ctx.stroke();

    // Points; last one highlighted.
    pts.forEach((p, i) => {
      const last = i === pts.length - 1;
      ctx.beginPath();
      ctx.arc(sx(p.x), sy(p.y), last ? 5 : 3, 0, Math.PI * 2);
      ctx.fillStyle = last ? "#3ddc84" : "#7fb2ff";
      ctx.fill();
    });
  }

  $effect(() => {
    // Re-run whenever the waypoint list changes.
    void pts.length;
    draw();
  });
</script>

<section class="cal">
  <header>
    <span>Calibration</span>
    <span class="count">{pts.length} waypoints</span>
    {#if $maslowState?.busy}<span class="live">● live</span>{/if}
  </header>

  {#if $calComplete}
    <div class="done">✓ Calibration complete</div>
  {/if}

  {#if active}
    <canvas bind:this={canvas} width="320" height="240"></canvas>
  {:else}
    <div class="idle">Waypoints appear here while calibration runs.</div>
  {/if}
</section>

<style>
  .cal {
    background: #161616;
    border: 1px solid #333;
    border-radius: 10px;
    padding: 0.7em 0.9em;
    display: flex;
    flex-direction: column;
    gap: 0.5em;
  }
  header {
    display: flex;
    align-items: center;
    gap: 0.7em;
    font-size: 0.85em;
    opacity: 0.85;
  }
  .count {
    font-size: 0.8em;
    opacity: 0.6;
  }
  .live {
    margin-left: auto;
    font-size: 0.78em;
    color: #3ddc84;
    animation: blink 1.2s steps(2) infinite;
  }
  @keyframes blink {
    50% {
      opacity: 0.3;
    }
  }
  .done {
    font-size: 0.85em;
    color: #3ddc84;
    background: #14301f;
    border: 1px solid #1f5a36;
    border-radius: 7px;
    padding: 0.4em 0.6em;
  }
  .idle {
    font-size: 0.8em;
    opacity: 0.45;
    padding: 0.2em 0;
  }
  canvas {
    display: block;
    width: 100%;
    /* Full-width waypoint map: never let the canvas push past its column and
       trigger a horizontal scrollbar. */
    max-width: 100%;
    height: auto;
    background: #0e0e0e;
    border: 1px solid #2a2a2a;
    border-radius: 8px;
  }
</style>
