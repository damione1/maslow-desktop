<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { wsState } from "$lib/stores/machine";
  import { maslowInfo, maslowState, actionPolicy, anchors, fullConfig } from "$lib/stores/maslow";
  import { isReadyToCut } from "$lib/stores/calState";
  import { CFG, configNumber } from "$lib/stores/config";
  import Button from "$lib/components/ui/Button.svelte";

  const connected = $derived($wsState === "connected");
  const policy = $derived($maslowState);
  const busy = $derived(policy?.busy ?? false);
  const info = $derived($maslowInfo);
  const calibrated = $derived($anchors?.calibrated ?? false);

  const ap = $derived($actionPolicy);
  const can = (key: keyof NonNullable<typeof ap>) => connected && (ap?.[key] ?? false);
  const settling = $derived(connected && busy);
  const canPark = $derived(connected && isReadyToCut(policy?.code) && (ap?.jog ?? false));
  const canDiag = $derived(connected && (ap?.jog ?? false));

  const CMD = {
    retract: "$ALL",
    extend: "$EXT",
    comply: "$CMP",
    takeSlack: "$TKSLK",
    calibrate: "$CAL",
    stop: "$STOP",
    estop: "$ESTOP",
    test: "$TEST",
    setZStop: "$SETZSTOP",
    calReset: "$CALRESET",
  } as const;

  function action(cmd: keyof typeof CMD) {
    invoke("send_line", { line: CMD[cmd] });
  }

  function calibrate() {
    if (
      !window.confirm(
        "Start calibration? The machine will drive to every measurement waypoint across the work area.",
      )
    )
      return;
    action("calibrate");
  }

  async function park() {
    const z = configNumber($fullConfig, CFG.parkZ, 2.0);
    const x = configNumber($fullConfig, CFG.parkX, 0.0);
    const y = configNumber($fullConfig, CFG.parkY, 0.0);
    if (!window.confirm(`Park the machine? It will lift Z to ${z} then move to X${x} Y${y}.`)) return;
    await invoke("send_line", { line: `G90 G0 Z${z}` });
    await invoke("send_line", { line: `G53 G0 Y${y} X${x}` });
  }

  function estopMaslow() {
    if (
      !window.confirm(
        "Trigger the latching emergency stop? The machine will not respond until you power it off and on again.",
      )
    )
      return;
    action("estop");
  }

  let showDiag = $state(false);
  function diag(cmd: "test" | "setZStop" | "calReset", confirmMsg?: string) {
    if (confirmMsg && !window.confirm(confirmMsg)) return;
    action(cmd);
  }

  const belts = $derived(
    info
      ? [
          { label: "TL", len: info.tl, err: info.etl },
          { label: "TR", len: info.tr, err: info.etr },
          { label: "BL", len: info.bl, err: info.ebl },
          { label: "BR", len: info.br, err: info.ebr },
        ]
      : [],
  );
  const fmt = (n: number | null) => (n === null ? "?" : n.toFixed(1));
</script>

<div class="belts-panel">
  <div class="head">
    <span class="state" class:busy class:ready={isReadyToCut(policy?.code)}>{policy?.label ?? "—"}</span>
    {#if info}
      <span class="flags">
        <span class:on={calibrated}>calibrated</span>
        <span class:on={info.homed}>homed</span>
        <span class:on={info.extended}>extended</span>
        {#if info.calibrationInProgress}<span class="on cal">calibrating</span>{/if}
      </span>
    {/if}
  </div>

  {#if belts.length > 0}
    <div class="belt-grid">
      {#each belts as b}
        <div class="belt" class:bad={b.len === null || b.err === null}>
          <span class="bl">{b.label}</span>
          <span class="len">{fmt(b.len)}<small>mm</small></span>
          <span class="err" class:warn={b.err !== null && Math.abs(b.err) > 1} class:bad={b.err === null}>{b.err !== null && b.err >= 0 ? "+" : ""}{fmt(b.err)}</span>
        </div>
      {/each}
    </div>
  {:else}
    <div class="hint">{connected ? "waiting for telemetry…" : "connect to view belts"}</div>
  {/if}

  {#if settling}
    <div class="settling">
      Machine settling… <strong>Retract</strong> returns it to a known state if it stays stuck.
    </div>
  {/if}

  <div class="grid-2">
    <Button variant={settling && can("retract") ? "active" : "action"} disabled={!can("retract")} title="Pull all belts fully in ($ALL)" onclick={() => action("retract")}>Retract</Button>
    <Button variant="action" disabled={!can("extend")} title="Pay all belts out ($EXT)" onclick={() => action("extend")}>Extend</Button>
    <Button variant="action" disabled={!can("comply")} title="Release belt tension ($CMP)" onclick={() => action("comply")}>Release tension</Button>
    <Button variant="action" disabled={!can("take_slack")} title="Tension the belts ($TKSLK)" onclick={() => action("takeSlack")}>Take slack</Button>
  </div>

  <div class="grid-2">
    <Button variant="active" disabled={!can("calibrate")} title="Run the measurement grid ($CAL)" onclick={calibrate}>Calibrate</Button>
    <Button variant="action" disabled={!canPark} title="Lift Z and move to the configured park position" onclick={park}>Park</Button>
  </div>

  <div class="grid-2">
    <Button variant="datum" disabled={!can("stop")} title="Stop motors and cancel calibration — recoverable ($STOP)" onclick={() => action("stop")}>Stop</Button>
    <Button variant="danger" disabled={!can("estop")} title="Latching emergency stop — requires power cycle ($ESTOP)" onclick={estopMaslow}>E-Stop ⚠</Button>
  </div>

  <details class="diag" bind:open={showDiag}>
    <summary>Diagnostics</summary>
    <div class="grid-3">
      <Button variant="ghost" size="sm" disabled={!canDiag} title="Run the Maslow self-test ($TEST)" onclick={() => diag("test", "Run the motor/sensor self-test? The machine will move.")}>Test</Button>
      <Button variant="ghost" size="sm" disabled={!canDiag} title="Set the current Z position as the Z stop ($SETZSTOP)" onclick={() => diag("setZStop")}>Set Z stop</Button>
      <Button variant="datum" size="sm" disabled={!canDiag} title="Reset the calibration state ($CALRESET)" onclick={() => diag("calReset", "Reset the calibration state machine? Use this to recover a stuck calibration.")}>Reset cal</Button>
    </div>
  </details>
</div>

<style>
  .belts-panel {
    display: flex;
    flex-direction: column;
    gap: var(--gap);
  }
  .head {
    display: flex;
    align-items: center;
    gap: var(--gap);
    flex-wrap: wrap;
  }
  .state {
    font-weight: 700;
    padding: 0.25em 0.8em;
    border-radius: var(--radius);
    background: var(--surface-3);
  }
  .state.busy {
    background: var(--state-hold);
    animation: pulse 1.4s ease-in-out infinite;
  }
  .state.ready {
    background: var(--state-idle);
  }
  @keyframes pulse {
    50% {
      opacity: 0.55;
    }
  }
  .flags {
    margin-left: auto;
    display: flex;
    gap: 0.6em;
    font-size: 0.75em;
  }
  .flags span {
    opacity: 0.35;
    text-transform: uppercase;
    letter-spacing: 0.03em;
  }
  .flags span.on {
    opacity: 1;
    color: #3ddc84;
  }
  .flags span.cal {
    color: var(--warn);
  }
  .belt-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--gap-sm);
  }
  .belt {
    display: flex;
    align-items: baseline;
    gap: 0.5em;
    background: var(--surface-2);
    border-radius: var(--radius);
    padding: 0.5em 0.7em;
  }
  .belt.bad {
    outline: 1px solid var(--warn);
  }
  .bl {
    font-size: 0.75em;
    color: var(--text-mute);
    width: 22px;
  }
  .len {
    font-size: 1.1em;
    font-weight: 600;
    font-family: var(--mono);
    font-variant-numeric: tabular-nums;
  }
  .len small {
    font-size: 0.6em;
    opacity: 0.5;
    margin-left: 2px;
  }
  .err {
    margin-left: auto;
    font-size: 0.8em;
    color: var(--text-mute);
    font-variant-numeric: tabular-nums;
  }
  .err.warn {
    color: #ff6b6b;
  }
  .err.bad {
    color: var(--warn);
  }
  .hint {
    font-size: 0.85em;
    color: var(--text-mute);
    padding: 0.3em 0;
  }
  .settling {
    font-size: 0.8em;
    line-height: 1.35;
    color: var(--warn);
    background: #2a2008;
    border: 1px solid #6b4a1f;
    border-radius: var(--radius);
    padding: 0.5em 0.7em;
  }
  .settling strong {
    color: #ffd166;
  }
  .grid-2 {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--gap-sm);
  }
  .grid-3 {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: var(--gap-sm);
    margin-top: var(--gap-sm);
  }
  .diag {
    border-top: 1px solid var(--border);
    padding-top: 0.5em;
  }
  .diag summary {
    cursor: pointer;
    color: var(--text-dim);
    font-size: 0.85em;
    user-select: none;
  }
</style>
