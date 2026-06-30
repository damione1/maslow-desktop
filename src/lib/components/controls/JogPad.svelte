<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { wsState, machineStatus } from "$lib/stores/machine";
  import { actionPolicy, maslowInfo } from "$lib/stores/maslow";
  import Button from "$lib/components/ui/Button.svelte";
  import OverrideControl from "$lib/components/ui/OverrideControl.svelte";

  const DISTS = [0.1, 1, 10, 100];
  // Named jog-speed presets (mm/min). Med matches the prior default feed.
  const SPEEDS = [
    { id: "slow", label: "Slow", feed: 250 },
    { id: "med", label: "Med", feed: 1000 },
    { id: "fast", label: "Fast", feed: 2500 },
    { id: "ultra", label: "Ultra", feed: 5000 },
  ];

  let dist = $state(10);
  let speedId = $state("med");
  const feed = $derived(SPEEDS.find((s) => s.id === speedId)?.feed ?? 1000);

  const connected = $derived($wsState === "connected");
  const ap = $derived($actionPolicy);
  const allow = (key: keyof NonNullable<typeof ap>) => connected && (ap?.[key] ?? false);
  const canJog = $derived(allow("jog"));
  // X/Y motion needs the belts paid out + tensioned; before that, jogging XY does
  // nothing useful, so gate it on the extended flag. Z is a separate leadscrew and
  // stays available whenever jogging is allowed.
  const extended = $derived($maslowInfo?.extended ?? false);
  const canJogXY = $derived(canJog && extended);

  function line(cmd: string) {
    invoke("send_line", { line: cmd });
  }
  function realtime(byte: number) {
    invoke("send_realtime", { byte });
  }
  function jog(axis: "X" | "Y" | "Z", dir: 1 | -1) {
    line(`$J=G91 G21 ${axis}${dir * dist} F${feed}`);
  }

  const feedOv = $derived($machineStatus?.ov?.[0] ?? 100);
</script>

<div class="jogpad">
  <div class="pads">
    <div class="xy" title={!extended && canJog ? "Extend the belts before jogging X/Y" : undefined}>
      <Button variant="action" size="lg" disabled={!canJogXY} onclick={() => jog("Y", 1)}>Y+</Button>
      <Button variant="action" size="lg" disabled={!canJogXY} onclick={() => jog("X", -1)}>X−</Button>
      <Button variant="ghost" size="lg" disabled={!connected} title="Jog cancel" onclick={() => realtime(0x85)}>◼</Button>
      <Button variant="action" size="lg" disabled={!canJogXY} onclick={() => jog("X", 1)}>X+</Button>
      <Button variant="action" size="lg" disabled={!canJogXY} onclick={() => jog("Y", -1)}>Y−</Button>
    </div>
    <div class="z">
      <Button variant="action" size="lg" disabled={!canJog} onclick={() => jog("Z", 1)}>Z+</Button>
      <span class="z-lbl">Z</span>
      <Button variant="action" size="lg" disabled={!canJog} onclick={() => jog("Z", -1)}>Z−</Button>
    </div>
  </div>

  <div class="selectors">
    <div class="sel">
      <span class="lbl">Distance (mm)</span>
      <div class="chips">
        {#each DISTS as d}
          <button class="chip" class:on={dist === d} onclick={() => (dist = d)}>{d}</button>
        {/each}
      </div>
    </div>
    <div class="sel">
      <span class="lbl">Speed</span>
      <div class="chips">
        {#each SPEEDS as s}
          <button class="chip" class:on={speedId === s.id} onclick={() => (speedId = s.id)}
            >{s.label}</button
          >
        {/each}
      </div>
    </div>
  </div>

  <div class="realtime">
    <Button variant="ghost" disabled={!allow("hold")} onclick={() => realtime(0x21)}>Hold !</Button>
    <Button variant="active" disabled={!allow("resume")} onclick={() => realtime(0x7e)}>Resume ~</Button>
  </div>

  <OverrideControl
    label="Feed override"
    value={feedOv}
    disabled={!connected}
    onUp={() => realtime(0x91)}
    onDown={() => realtime(0x92)}
    onReset={() => realtime(0x90)}
  />
</div>

<style>
  .jogpad {
    display: flex;
    flex-direction: column;
    gap: var(--gap);
  }
  .pads {
    display: flex;
    gap: var(--gap);
    justify-content: center;
    flex-wrap: wrap;
  }
  .xy {
    display: grid;
    grid-template-columns: repeat(3, minmax(72px, 1fr));
    grid-template-rows: repeat(3, var(--tap-lg));
    gap: var(--gap-sm);
  }
  /* Fill the grid cell so the centered label sits dead-center of each pad. */
  .xy :global(.btn),
  .z :global(.btn) {
    width: 100%;
    height: 100%;
    padding: 0;
  }
  .xy :global(.btn:nth-child(1)) {
    grid-column: 2;
    grid-row: 1;
  }
  .xy :global(.btn:nth-child(2)) {
    grid-column: 1;
    grid-row: 2;
  }
  .xy :global(.btn:nth-child(3)) {
    grid-column: 2;
    grid-row: 2;
  }
  .xy :global(.btn:nth-child(4)) {
    grid-column: 3;
    grid-row: 2;
  }
  .xy :global(.btn:nth-child(5)) {
    grid-column: 2;
    grid-row: 3;
  }
  .z {
    display: grid;
    grid-template-rows: var(--tap-lg) auto var(--tap-lg);
    gap: var(--gap-sm);
    align-items: center;
    width: 84px;
  }
  .z-lbl {
    text-align: center;
    color: var(--text-mute);
    font-size: 0.8em;
  }
  .selectors {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--gap);
  }
  .sel {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .lbl {
    font-size: 0.8em;
    color: var(--text-dim);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .chips {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 4px;
  }
  .chip {
    min-height: var(--tap);
    border: 1px solid var(--border-2);
    border-radius: var(--radius);
    background: var(--surface-2);
    color: var(--text);
    font-family: var(--mono);
    font-variant-numeric: tabular-nums;
    font-size: 0.95em;
    cursor: pointer;
  }
  .chip:hover {
    background: var(--surface-3);
  }
  .chip.on {
    background: var(--action);
    border-color: var(--action);
    color: var(--action-text);
  }
  .realtime {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--gap-sm);
  }
  @media (max-width: 560px) {
    .selectors {
      grid-template-columns: 1fr;
    }
  }
</style>
