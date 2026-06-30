<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { wsState, machineStatus } from "$lib/stores/machine";
  import { actionPolicy } from "$lib/stores/maslow";
  import { mainSubTab } from "$lib/stores/ui";
  import StateBar from "$lib/components/ui/StateBar.svelte";
  import AxisTable from "$lib/components/ui/AxisTable.svelte";
  import SubTabs from "$lib/components/ui/SubTabs.svelte";
  import Button from "$lib/components/ui/Button.svelte";
  import Modal from "$lib/components/ui/Modal.svelte";
  import JogPad from "$lib/components/controls/JogPad.svelte";
  import BeltControls from "$lib/components/controls/BeltControls.svelte";
  import Console from "$lib/components/Console.svelte";

  const connected = $derived($wsState === "connected");
  const ap = $derived($actionPolicy);
  const allow = (key: keyof NonNullable<typeof ap>) => connected && (ap?.[key] ?? false);
  const canZero = $derived(allow("zero"));
  const canHome = $derived(allow("home"));
  const canUnlock = $derived(allow("unlock"));
  const canGoHome = $derived(allow("jog"));

  function line(cmd: string) {
    invoke("send_line", { line: cmd });
  }

  function zeroAll() {
    if (!window.confirm("Zero all axes? Sets work X/Y/Z = 0 at the current position.")) return;
    line("G10 L20 P0 X0 Y0 Z0");
  }
  const homeAll = () => line("$H");
  const unlock = () => line("$X");

  // Per-axis "Go to home": move that axis to its work zero. Confirmed (it moves).
  function goAxisHome(_i: number, axis: string) {
    if (!window.confirm(`Go to ${axis} home? The machine will move ${axis} to work zero.`)) return;
    line(`G90 G0 ${axis}0`);
  }

  // Per-axis "Set home": open a dialog showing the current position and a value to
  // define for it. Defaults to 0 (zero this axis here) so confirming visibly sets
  // something; can be edited to any value for a touch-off.
  let axisSet = $state<{ axis: string; index: number } | null>(null);
  let axisValue = $state(0);
  function openAxisSet(i: number, axis: string) {
    axisSet = { axis, index: i };
    axisValue = 0;
  }
  function applyAxisSet() {
    if (!axisSet) return;
    line(`G10 L20 P0 ${axisSet.axis}${axisValue}`);
    axisSet = null;
  }
  const axisWork = $derived.by(() => {
    if (!axisSet) return "—";
    const s = $machineStatus;
    const arr = s ? (s.wpos.length ? s.wpos : s.mpos) : [];
    return arr[axisSet.index]?.toFixed(3) ?? "—";
  });
  const axisMachine = $derived(
    axisSet ? ($machineStatus?.mpos?.[axisSet.index]?.toFixed(3) ?? "—") : "—",
  );

  const SUBTABS = [
    { id: "jog", label: "Jog" },
    { id: "belts", label: "Belts" },
    { id: "mdi", label: "MDI" },
  ];
</script>

<div class="main-tab">
  <StateBar />
  <div class="cols">
    <div class="status-block">
      <div class="datum">
        <Button variant="datum" size="lg" disabled={!canZero} title="Set work X/Y/Z = 0 at the current position" onclick={zeroAll}
          >⌖ Zero all</Button
        >
        <Button variant="datum" size="lg" disabled={!canHome} title="Home all axes ($H)" onclick={homeAll}
          >⌂ Home all</Button
        >
        <Button variant="ghost" disabled={!canUnlock} title="Clear alarm lock ($X)" onclick={unlock}
          >Unlock</Button
        >
      </div>
      <AxisTable {canZero} canGoZero={canGoHome} onZero={openAxisSet} onGoZero={goAxisHome} />
    </div>

    <div class="sub">
      <SubTabs items={SUBTABS} bind:active={$mainSubTab} />
      <div class="sub-content">
        {#if $mainSubTab === "jog"}
          <JogPad />
        {:else if $mainSubTab === "belts"}
          <BeltControls />
        {:else}
          <div class="mdi"><Console /></div>
        {/if}
      </div>
    </div>
  </div>
</div>

{#if axisSet}
  <Modal title="Set {axisSet.axis} home" onclose={() => (axisSet = null)}>
    <p class="hint">
      Defines the work <strong>{axisSet.axis}</strong> for the current position. Leave it at
      <strong>0</strong> to zero the axis here, or enter a value for a touch-off.
    </p>
    <div class="current">
      <span>Current {axisSet.axis}</span>
      <span class="vals">work <strong>{axisWork}</strong> · machine {axisMachine}</span>
    </div>
    <div class="fields">
      <label>
        <span>Set work {axisSet.axis} to (mm)</span>
        <input type="number" step="0.1" bind:value={axisValue} />
      </label>
    </div>
    <div class="actions">
      <Button variant="ghost" onclick={() => (axisSet = null)}>Cancel</Button>
      <Button variant="active" disabled={!canZero} onclick={applyAxisSet}>Set {axisSet.axis} home</Button>
    </div>
  </Modal>
{/if}

<style>
  .main-tab {
    display: flex;
    flex-direction: column;
    gap: var(--gap);
    padding: var(--gap-lg);
  }
  .cols {
    display: grid;
    grid-template-columns: minmax(0, 360px) minmax(0, 1fr);
    gap: var(--gap-lg);
    align-items: start;
  }
  .status-block {
    display: flex;
    flex-direction: column;
    gap: var(--gap);
    min-width: 0;
  }
  .datum {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--gap-sm);
  }
  /* Unlock spans the full width on its own row. */
  .datum :global(.btn:nth-child(3)) {
    grid-column: 1 / -1;
  }
  .sub {
    display: flex;
    flex-direction: column;
    gap: var(--gap);
    min-width: 0;
  }
  .sub-content {
    background: var(--surface);
    border: 1px solid var(--border-2);
    border-radius: var(--radius-lg);
    padding: var(--gap-lg);
    min-height: 0;
  }
  .mdi {
    height: 420px;
  }

  .hint {
    margin: 0 0 0.6em;
    font-size: 0.85em;
    color: var(--text-dim);
    line-height: 1.4;
  }
  .current {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.8em;
    padding: 0.5em 0.7em;
    margin-bottom: 0.8em;
    background: var(--surface-2);
    border: 1px solid var(--border-2);
    border-radius: var(--radius);
    font-size: 0.85em;
    color: var(--text-dim);
  }
  .current .vals {
    font-family: var(--mono);
    font-variant-numeric: tabular-nums;
    color: var(--text);
  }
  .fields {
    display: flex;
    flex-direction: column;
    gap: 0.5em;
    margin-bottom: 0.8em;
  }
  .fields label {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.8em;
  }
  .fields input {
    width: 140px;
    padding: 0.5em 0.6em;
    border-radius: var(--radius);
    border: 1px solid var(--border-3);
    background: var(--surface-3);
    color: #fff;
    font-family: var(--mono);
    font-variant-numeric: tabular-nums;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--gap-sm);
  }

  /* Single column on narrow/touch viewports: status block on top, controls below. */
  @media (max-width: 860px) {
    .cols {
      grid-template-columns: 1fr;
    }
  }
</style>
