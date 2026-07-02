<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { wsState, machineStatus } from "$lib/stores/machine";
  import { jobProgress, toolpath, toolpathPath } from "$lib/stores/job";
  import { actionPolicy, fullConfig } from "$lib/stores/maslow";
  import { CFG, configNumber } from "$lib/stores/config";

  let canvas: HTMLCanvasElement | undefined = $state();
  let wrap: HTMLDivElement | undefined = $state();

  // Two views, like the embedded UI: the job on its own (auto-fit to the cut
  // extent) or the job placed inside the full machine work area.
  type View = "path" | "machine";
  let view = $state<View>("path");

  const tp = $derived($toolpath);
  // Work-area geometry, read from the discovered machine config by path.
  const workAreaX = $derived(configNumber($fullConfig, CFG.workAreaX, 0));
  const workAreaY = $derived(configNumber($fullConfig, CFG.workAreaY, 0));
  const connected = $derived($wsState === "connected");
  const jobActive = $derived(
    $jobProgress?.state === "running" || $jobProgress?.state === "paused",
  );
  const canTrace = $derived(
    connected && !jobActive && ($actionPolicy?.jog ?? false) && (tp?.has_bounds ?? false),
  );
  const hasMachine = $derived(workAreaX > 0 && workAreaY > 0);

  const dims = $derived(
    tp?.has_bounds ? { w: tp.max_x - tp.min_x, h: tp.max_y - tp.min_y } : null,
  );

  // Live tool position in work coordinates (same frame as the G-code).
  const tool = $derived.by(() => {
    const s = $machineStatus;
    if (!s || s.wpos.length < 2) return null;
    return { x: s.wpos[0], y: s.wpos[1] };
  });

  // Number of source lines confirmed cut, only when the running job matches the
  // previewed file (else don't highlight a stale preview).
  const progressLine = $derived.by(() => {
    const j = $jobProgress;
    if (!j || ($toolpathPath && j.path !== $toolpathPath)) return null;
    if (j.state !== "running" && j.state !== "paused" && j.state !== "interrupted")
      return null;
    return j.acked;
  });

  // The machine work-area rectangle in work coords (centered on the configured
  // offset), mirroring the firmware's workAreaX/Y + center offset.
  const machineRect = $derived.by(() => {
    if (!hasMachine) return null;
    const cx = configNumber($fullConfig, CFG.workAreaCenterOffsetX, 0);
    const cy = configNumber($fullConfig, CFG.workAreaCenterOffsetY, 0);
    return {
      minx: cx - workAreaX / 2,
      maxx: cx + workAreaX / 2,
      miny: cy - workAreaY / 2,
      maxy: cy + workAreaY / 2,
    };
  });

  // A fast-streaming job can emit stream-progress many times per second; coalesce
  // redraws to at most one per animation frame so a burst of acks doesn't repaint
  // the whole path (every segment) once per event.
  let rafScheduled = false;
  function scheduleDraw() {
    if (rafScheduled) return;
    rafScheduled = true;
    requestAnimationFrame(() => {
      rafScheduled = false;
      draw();
    });
  }

  // Keep the backing store matched to the rendered box (responsive + HiDPI).
  $effect(() => {
    if (!canvas || !wrap) return;
    const ro = new ResizeObserver(() => sizeAndDraw());
    ro.observe(wrap);
    sizeAndDraw();
    return () => ro.disconnect();
  });

  // Redraw on any data change.
  $effect(() => {
    void [$toolpath, progressLine, view, tool, machineRect];
    scheduleDraw();
  });

  function sizeAndDraw() {
    if (!canvas || !wrap) return;
    const dpr = window.devicePixelRatio || 1;
    const w = Math.max(1, Math.floor(wrap.clientWidth));
    const h = Math.max(1, Math.floor(w / 2)); // 2:1 like the embedded viewer
    canvas.style.height = `${h}px`;
    canvas.width = Math.floor(w * dpr);
    canvas.height = Math.floor(h * dpr);
    scheduleDraw();
  }

  function draw() {
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    const W = canvas.width;
    const H = canvas.height;
    const dpr = window.devicePixelRatio || 1;
    ctx.clearRect(0, 0, W, H);
    const t = $toolpath;
    if (!t || t.segments.length === 0) return;

    // View bounds: every segment endpoint, plus the machine rectangle in
    // "machine" view so the job is shown within the full work area.
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
    const mr = machineRect;
    if (view === "machine" && mr) {
      minx = Math.min(minx, mr.minx);
      miny = Math.min(miny, mr.miny);
      maxx = Math.max(maxx, mr.maxx);
      maxy = Math.max(maxy, mr.maxy);
    }

    const w = maxx - minx || 1;
    const h = maxy - miny || 1;
    const pad = 16 * dpr;
    const s = Math.min((W - 2 * pad) / w, (H - 2 * pad) / h);
    const ox = pad + (W - 2 * pad - w * s) / 2;
    const oy = pad + (H - 2 * pad - h * s) / 2;
    const X = (px: number) => ox + (px - minx) * s;
    const Y = (py: number) => H - (oy + (py - miny) * s);

    // Machine work-area rectangle (green) in machine view.
    if (view === "machine" && mr) {
      ctx.strokeStyle = "#2e7d32";
      ctx.lineWidth = 1.5 * dpr;
      ctx.strokeRect(X(mr.minx), Y(mr.maxy), (mr.maxx - mr.minx) * s, (mr.maxy - mr.miny) * s);
      // Work origin crosshair.
      ctx.strokeStyle = "#b85c5c";
      ctx.lineWidth = 1 * dpr;
      const r = 6 * dpr;
      ctx.beginPath();
      ctx.moveTo(X(0) - r, Y(0));
      ctx.lineTo(X(0) + r, Y(0));
      ctx.moveTo(X(0), Y(0) - r);
      ctx.lineTo(X(0), Y(0) + r);
      ctx.stroke();
    }

    // Cutting-extent bounding box (dashed amber).
    if (t.has_bounds) {
      ctx.strokeStyle = "#7a6a3a";
      ctx.setLineDash([4 * dpr, 3 * dpr]);
      ctx.lineWidth = 1 * dpr;
      ctx.strokeRect(X(t.min_x), Y(t.max_y), (t.max_x - t.min_x) * s, (t.max_y - t.min_y) * s);
      ctx.setLineDash([]);
    }

    const cutting = progressLine != null;
    const isDone = (sg: (typeof t.segments)[number]) => cutting && sg.line < progressLine!;

    // Rapids (dim, dashed).
    ctx.lineWidth = 1 * dpr;
    ctx.beginPath();
    for (const sg of t.segments) {
      if (!sg.rapid) continue;
      ctx.moveTo(X(sg.x0), Y(sg.y0));
      ctx.lineTo(X(sg.x1), Y(sg.y1));
    }
    ctx.strokeStyle = "#444";
    ctx.setLineDash([3 * dpr, 3 * dpr]);
    ctx.stroke();
    ctx.setLineDash([]);

    // Pending feed moves — dimmed while a job runs so the cut part stands out.
    ctx.beginPath();
    for (const sg of t.segments) {
      if (sg.rapid || isDone(sg)) continue;
      ctx.moveTo(X(sg.x0), Y(sg.y0));
      ctx.lineTo(X(sg.x1), Y(sg.y1));
    }
    ctx.strokeStyle = cutting ? "#3a4a63" : "#6ea8fe";
    ctx.lineWidth = 1.3 * dpr;
    ctx.stroke();

    // Cut-so-far feed moves — bright green on top.
    if (cutting) {
      ctx.beginPath();
      for (const sg of t.segments) {
        if (sg.rapid || !isDone(sg)) continue;
        ctx.moveTo(X(sg.x0), Y(sg.y0));
        ctx.lineTo(X(sg.x1), Y(sg.y1));
      }
      ctx.strokeStyle = "#7ee08a";
      ctx.lineWidth = 2 * dpr;
      ctx.stroke();
    }

    // Live tool position (magenta dot), driven by reported work position.
    if (tool) {
      ctx.fillStyle = "#e368d8";
      ctx.beginPath();
      ctx.arc(X(tool.x), Y(tool.y), 4 * dpr, 0, Math.PI * 2);
      ctx.fill();
      ctx.strokeStyle = "#1a1a1a";
      ctx.lineWidth = 1 * dpr;
      ctx.stroke();
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

    <div class="views">
      <button class:on={view === "path"} onclick={() => (view = "path")}>Path</button>
      <button
        class:on={view === "machine"}
        onclick={() => (view = "machine")}
        disabled={!hasMachine}
        title={hasMachine ? "Show the job inside the machine work area" : "Work-area size unknown — read the Maslow config first"}
      >
        Machine
      </button>
    </div>

    <button class="trace" onclick={trace} disabled={!canTrace} title="Move around the job bounding box">
      Trace boundary
    </button>
  </header>

  <div class="wrap" bind:this={wrap}>
    <canvas bind:this={canvas}></canvas>
  </div>

  {#if !tp || tp.segments.length === 0}
    <div class="hint">Load a local or SD-card G-code file to preview its toolpath.</div>
  {:else}
    <div class="legend">
      <span class="feed">— cut</span>
      <span class="rapid">— rapid</span>
      <span class="box">▢ bounds</span>
      {#if view === "machine"}<span class="mach">▢ work area</span>{/if}
      {#if tool}<span class="tool">● tool</span>{/if}
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
  .views {
    margin-left: auto;
    display: flex;
    gap: 0;
    border: 1px solid #3a3a3a;
    border-radius: 6px;
    overflow: hidden;
  }
  .views button {
    background: #222;
    color: #cfcfcf;
    border: none;
    padding: 0.3em 0.7em;
    cursor: pointer;
    font-size: 0.82em;
  }
  .views button.on {
    background: #396cd8;
    color: #fff;
  }
  .views button:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
  .trace {
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
  .wrap {
    width: 100%;
  }
  canvas {
    display: block;
    width: 100%;
    background: #111;
    border: 1px solid #262626;
    border-radius: 6px;
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
    flex-wrap: wrap;
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
  .legend .mach {
    color: #2e7d32;
  }
  .legend .tool {
    color: #e368d8;
  }
</style>
