<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { wsState } from "$lib/stores/machine";
  import { jobProgress, toolpath, toolpathPath } from "$lib/stores/job";
  import { actionPolicy } from "$lib/stores/maslow";

  const W = 360;
  const H = 260;
  let canvas: HTMLCanvasElement;

  const tp = $derived($toolpath);
  const connected = $derived($wsState === "connected");
  const jobActive = $derived(
    $jobProgress?.state === "running" || $jobProgress?.state === "paused",
  );
  // Idle motion is allowed (FluidNC Idle, no job) → jog flag from the policy.
  const canTrace = $derived(
    connected && !jobActive && ($actionPolicy?.jog ?? false) && (tp?.has_bounds ?? false),
  );

  const dims = $derived(
    tp?.has_bounds
      ? { w: tp.max_x - tp.min_x, h: tp.max_y - tp.min_y }
      : null,
  );

  // Number of source lines confirmed cut, but only when the running job is the
  // file this toolpath was parsed from (else don't highlight a stale preview).
  const progressLine = $derived.by(() => {
    const j = $jobProgress;
    if (!j || ($toolpathPath && j.path !== $toolpathPath)) return null;
    if (j.state !== "running" && j.state !== "paused" && j.state !== "interrupted")
      return null;
    return j.acked;
  });

  $effect(() => {
    draw($toolpath, progressLine);
  });

  function draw(t: typeof $toolpath, done: number | null) {
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.clearRect(0, 0, W, H);
    if (!t || t.segments.length === 0) return;

    // View bounds over every segment endpoint (rapids included) so nothing is
    // clipped, independent of the cutting-only bounding box.
    let minx = Infinity,
      miny = Infinity,
      maxx = -Infinity,
      maxy = -Infinity;
    for (const s of t.segments) {
      minx = Math.min(minx, s.x0, s.x1);
      miny = Math.min(miny, s.y0, s.y1);
      maxx = Math.max(maxx, s.x0, s.x1);
      maxy = Math.max(maxy, s.y0, s.y1);
    }
    const w = maxx - minx || 1;
    const h = maxy - miny || 1;
    const pad = 14;
    const s = Math.min((W - 2 * pad) / w, (H - 2 * pad) / h);
    const ox = pad + (W - 2 * pad - w * s) / 2;
    const oy = pad + (H - 2 * pad - h * s) / 2;
    // Map gcode (x,y) → canvas px, flipping Y (gcode Y up, canvas Y down).
    const X = (px: number) => ox + (px - minx) * s;
    const Y = (py: number) => H - (oy + (py - miny) * s);

    // Cutting-extent bounding box (dashed amber).
    if (t.has_bounds) {
      ctx.strokeStyle = "#7a6a3a";
      ctx.setLineDash([4, 3]);
      ctx.lineWidth = 1;
      ctx.strokeRect(
        X(t.min_x),
        Y(t.max_y),
        (t.max_x - t.min_x) * s,
        (t.max_y - t.min_y) * s,
      );
      ctx.setLineDash([]);
    }

    const cutting = done != null;
    const isDone = (sg: (typeof t.segments)[number]) => cutting && sg.line < done!;

    // Rapids (dim, dashed).
    ctx.lineWidth = 1;
    ctx.beginPath();
    for (const sg of t.segments) {
      if (!sg.rapid) continue;
      ctx.moveTo(X(sg.x0), Y(sg.y0));
      ctx.lineTo(X(sg.x1), Y(sg.y1));
    }
    ctx.strokeStyle = "#444";
    ctx.setLineDash([3, 3]);
    ctx.stroke();
    ctx.setLineDash([]);

    // Pending feed moves — dimmed while a job is running so the cut part stands out.
    ctx.beginPath();
    for (const sg of t.segments) {
      if (sg.rapid || isDone(sg)) continue;
      ctx.moveTo(X(sg.x0), Y(sg.y0));
      ctx.lineTo(X(sg.x1), Y(sg.y1));
    }
    ctx.strokeStyle = cutting ? "#3a4a63" : "#6ea8fe";
    ctx.stroke();

    // Cut-so-far feed moves — bright green, drawn on top.
    if (cutting) {
      ctx.beginPath();
      let cur: [number, number] | null = null;
      for (const sg of t.segments) {
        if (sg.rapid || !isDone(sg)) continue;
        ctx.moveTo(X(sg.x0), Y(sg.y0));
        ctx.lineTo(X(sg.x1), Y(sg.y1));
        cur = [sg.x1, sg.y1];
      }
      ctx.strokeStyle = "#7ee08a";
      ctx.lineWidth = 1.7;
      ctx.stroke();
      ctx.lineWidth = 1;
      // Current position marker at the end of the last cut segment.
      if (cur) {
        ctx.fillStyle = "#d8f0a0";
        ctx.beginPath();
        ctx.arc(X(cur[0]), Y(cur[1]), 3, 0, Math.PI * 2);
        ctx.fill();
      }
    }
  }

  async function trace() {
    const t = tp;
    if (!t?.has_bounds) return;
    const wmm = (t.max_x - t.min_x).toFixed(0);
    const hmm = (t.max_y - t.min_y).toFixed(0);
    if (
      !window.confirm(
        `Trace boundary? The machine will move around the ${wmm}×${hmm} mm job perimeter at the current Z (Z does not move).`,
      )
    )
      return;
    const f = (n: number) => n.toFixed(3);
    const cmds = [
      "G21 G90",
      `G0 X${f(t.min_x)} Y${f(t.min_y)}`,
      `G0 X${f(t.max_x)} Y${f(t.min_y)}`,
      `G0 X${f(t.max_x)} Y${f(t.max_y)}`,
      `G0 X${f(t.min_x)} Y${f(t.max_y)}`,
      `G0 X${f(t.min_x)} Y${f(t.min_y)}`,
    ];
    for (const c of cmds) await invoke("send_line", { line: c });
  }
</script>

<section class="tp">
  <header>
    <span>Toolpath</span>
    {#if progressLine !== null && $jobProgress?.total}
      <span class="cutting">
        ● {Math.round(($jobProgress.acked / $jobProgress.total) * 100)}% cut
      </span>
    {:else if dims}
      <span class="dims">{dims.w.toFixed(1)} × {dims.h.toFixed(1)} mm</span>
    {/if}
    <button class="trace" onclick={trace} disabled={!canTrace} title="Move around the job bounding box">
      Trace boundary
    </button>
  </header>

  <canvas bind:this={canvas} width={W} height={H}></canvas>

  {#if !tp || tp.segments.length === 0}
    <div class="hint">Select a local G-code file to preview its toolpath.</div>
  {:else}
    <div class="legend">
      <span class="feed">— cut</span>
      <span class="rapid">— rapid</span>
      <span class="box">▢ bounds</span>
    </div>
  {/if}
</section>

<style>
  .tp {
    border: 1px solid #2a2a2a;
    border-radius: 8px;
    background: #1a1a1a;
    padding: 0.7em 0.8em 0.8em;
    display: flex;
    flex-direction: column;
    gap: 0.5em;
  }
  header {
    display: flex;
    align-items: center;
    gap: 0.6em;
  }
  header > span:first-child {
    font-weight: 600;
  }
  .dims {
    color: #9a9a9a;
    font-size: 0.85em;
    font-variant-numeric: tabular-nums;
  }
  .cutting {
    color: #7ee08a;
    font-size: 0.85em;
    font-variant-numeric: tabular-nums;
  }
  .trace {
    margin-left: auto;
    background: #45403d;
    color: #fff;
    border: 1px solid #5a5a5a;
    border-radius: 5px;
    padding: 0.3em 0.7em;
    cursor: pointer;
  }
  .trace:disabled {
    opacity: 0.45;
    cursor: default;
  }
  canvas {
    width: 100%;
    max-width: 360px;
    height: auto;
    background: #111;
    border: 1px solid #262626;
    border-radius: 6px;
    align-self: center;
  }
  .hint {
    color: #9a9a9a;
    font-size: 0.9em;
  }
  .legend {
    display: flex;
    gap: 1em;
    font-size: 0.8em;
    color: #9a9a9a;
  }
  .legend .feed {
    color: #6ea8fe;
  }
  .legend .rapid {
    color: #777;
  }
  .legend .box {
    color: #7a6a3a;
  }
</style>
