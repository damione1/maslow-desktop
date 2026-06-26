<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { wsState } from "$lib/stores/machine";
  import { jobProgress } from "$lib/stores/job";
  import { actionPolicy } from "$lib/stores/maslow";

  // `touch` enlarges the jog pad + controls for shop-floor finger use on
  // phone/tablet; the compact desktop rail leaves it off.
  let { touch = false }: { touch?: boolean } = $props();

  const STEPS = [0.1, 1, 10, 50];
  let step = $state(10);
  let feed = $state(1000);

  const connected = $derived($wsState === "connected");
  const jobActive = $derived(
    $jobProgress !== null &&
      $jobProgress.state !== "done" &&
      $jobProgress.state !== "error",
  );
  // Gating from the unified action policy (FluidNC state + job).
  const ap = $derived($actionPolicy);
  const allow = (key: keyof NonNullable<typeof ap>) =>
    connected && (ap?.[key] ?? false);
  const canJog = $derived(allow("jog"));
  const canHome = $derived(allow("home"));
  const canUnlock = $derived(allow("unlock"));
  const canZero = $derived(allow("zero"));

  function line(cmd: string) {
    invoke("send_line", { line: cmd });
  }

  function realtime(byte: number) {
    invoke("send_realtime", { byte });
  }

  function jog(axis: "X" | "Y" | "Z", dir: 1 | -1) {
    const dist = (dir * step).toString();
    line(`$J=G91 G21 ${axis}${dist} F${feed}`);
  }

  // 0x18 = Ctrl-X soft reset, 0x21 = '!', 0x7e = '~', 0x85 = jog cancel.
  const hold = () => realtime(0x21);
  const resume = () => realtime(0x7e);
  const reset = () => realtime(0x18);
  const jogCancel = () => realtime(0x85);

  const home = () => line("$H");
  const unlock = () => line("$X");
  const zeroAll = () => line("G10 L20 P0 X0 Y0 Z0");
  const zeroZ = () => line("G10 L20 P0 Z0");
</script>

<section class="jog" class:touch>
  <header>
    <span>Manual Control</span>
    {#if jobActive}<span class="hint">locked during job</span>{/if}
  </header>

  <div class="grid">
    <div class="xy">
      <button class="up" onclick={() => jog("Y", 1)} disabled={!canJog}>Y+</button>
      <button class="left" onclick={() => jog("X", -1)} disabled={!canJog}>X−</button>
      <button class="home" onclick={home} disabled={!canHome} title="Home $H">⌂</button>
      <button class="right" onclick={() => jog("X", 1)} disabled={!canJog}>X+</button>
      <button class="down" onclick={() => jog("Y", -1)} disabled={!canJog}>Y−</button>
    </div>

    <div class="z">
      <button onclick={() => jog("Z", 1)} disabled={!canJog}>Z+</button>
      <button onclick={() => jog("Z", -1)} disabled={!canJog}>Z−</button>
    </div>
  </div>

  <div class="row steps">
    <div class="group">
      <span class="lbl">Step</span>
      <div class="chips">
        {#each STEPS as s}
          <button class="chip" class:on={step === s} onclick={() => (step = s)}>
            {s}
          </button>
        {/each}
      </div>
    </div>
    <label class="group">
      <span class="lbl">Feed</span>
      <input class="feed" type="number" min="1" bind:value={feed} />
    </label>
  </div>

  <div class="row">
    <button class="ghost" onclick={unlock} disabled={!canUnlock}>Unlock $X</button>
    <button class="ghost" onclick={zeroAll} disabled={!canZero}>Zero XYZ</button>
    <button class="ghost" onclick={zeroZ} disabled={!canZero} title="Touch off Z (G10 L20 P0 Z0)">Zero Z</button>
    <button class="ghost" onclick={jogCancel} disabled={!connected}>Jog Cancel</button>
  </div>

  <div class="row realtime">
    <button class="hold" onclick={hold} disabled={!connected}>Hold !</button>
    <button class="resume" onclick={resume} disabled={!connected}>Resume ~</button>
    <button class="danger" onclick={reset} disabled={!connected}>Reset ⌃X</button>
  </div>
</section>

<style>
  .jog {
    background: #161616;
    border: 1px solid #333;
    border-radius: 10px;
    padding: 0.7em 0.9em;
    display: flex;
    flex-direction: column;
    gap: 0.7em;
  }
  header {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    font-size: 0.85em;
    opacity: 0.85;
  }
  .hint {
    font-size: 0.78em;
    color: #e0a83d;
  }
  .grid {
    display: flex;
    gap: 1.2em;
    align-items: center;
  }
  .xy {
    display: grid;
    grid-template-columns: repeat(3, 48px);
    grid-template-rows: repeat(3, 40px);
    gap: 6px;
  }
  .xy .up { grid-column: 2; grid-row: 1; }
  .xy .left { grid-column: 1; grid-row: 2; }
  .xy .home { grid-column: 2; grid-row: 2; }
  .xy .right { grid-column: 3; grid-row: 2; }
  .xy .down { grid-column: 2; grid-row: 3; }
  .z {
    display: grid;
    grid-template-rows: 40px 40px;
    gap: 6px;
    width: 48px;
  }
  button {
    border-radius: 8px;
    border: 1px solid #396cd8;
    background: #2b3a5c;
    color: #fff;
    cursor: pointer;
    font-size: 0.9em;
  }
  button:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
  .home {
    background: #333;
    border-color: #555;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 0.5em;
    flex-wrap: wrap;
  }
  .row button {
    padding: 0.4em 0.9em;
  }
  .steps {
    gap: 1em;
    row-gap: 0.6em;
  }
  .group {
    display: flex;
    align-items: center;
    gap: 0.5em;
  }
  .chips {
    display: flex;
    align-items: center;
    gap: 0.5em;
  }
  .lbl {
    font-size: 0.78em;
    opacity: 0.6;
  }
  .chip {
    background: #222;
    border-color: #444;
    padding: 0.3em 0.7em;
    font-variant-numeric: tabular-nums;
  }
  .chip.on {
    background: #396cd8;
    border-color: #396cd8;
  }
  .feed {
    width: 80px;
    padding: 0.35em 0.5em;
    border-radius: 7px;
    border: 1px solid #444;
    background: #2b2b2b;
    color: #fff;
  }
  .ghost {
    background: transparent;
    border-color: #555;
  }
  .realtime .hold {
    background: #b8860b;
    border-color: #b8860b;
  }
  .realtime .resume {
    background: #2e7d32;
    border-color: #2e7d32;
  }
  .danger {
    background: #8b2e2e;
    border-color: #8b2e2e;
  }

  /* Touch mode (phone/tablet): jog pad sized for fingers — ~15mm targets per
     industrial HMI guidance — and the pad centred so both thumbs reach it. */
  .jog.touch {
    gap: 1em;
  }
  .jog.touch header {
    font-size: 1em;
  }
  .jog.touch .grid {
    justify-content: center;
    gap: 1.6em;
  }
  .jog.touch .xy {
    grid-template-columns: repeat(3, 76px);
    grid-template-rows: repeat(3, 68px);
    gap: 10px;
  }
  .jog.touch .z {
    grid-template-rows: 68px 68px;
    width: 76px;
    gap: 10px;
  }
  .jog.touch button {
    font-size: 1.1em;
  }
  .jog.touch .row button {
    min-height: 48px;
    padding: 0.5em 1em;
    flex: 1;
  }
  .jog.touch .chip {
    min-height: 44px;
    min-width: 52px;
    padding: 0.5em 0.9em;
    font-size: 1em;
    flex: 1;
  }
  .jog.touch .chips {
    flex: 1;
  }
  .jog.touch .feed {
    width: 100px;
    min-height: 44px;
  }
  .jog.touch .steps {
    flex-wrap: wrap;
  }
</style>
