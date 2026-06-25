<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { wsState } from "$lib/stores/machine";
  import {
    maslowInfo,
    maslowState,
    actionPolicy,
    anchors,
    maslowConfig,
  } from "$lib/stores/maslow";

  const connected = $derived($wsState === "connected");
  const policy = $derived($maslowState);
  const busy = $derived(policy?.busy ?? false);
  const info = $derived($maslowInfo);
  const calibrated = $derived($anchors?.valid ?? false);

  // Allowed actions come from the unified Rust action policy (reconciles the
  // FluidNC state + the Maslow calibration state + any running job).
  const ap = $derived($actionPolicy);
  const can = (key: keyof NonNullable<typeof ap>) =>
    connected && (ap?.[key] ?? false);
  const canRetract = $derived(can("retract"));
  const canExtend = $derived(can("extend"));
  // While the machine is busy/transitional (e.g. it got stuck after a Stop in
  // EXTENDING, where FluidNC reads Idle but the Maslow FSM stays put), Retract
  // is the firmware's only accepted recovery. Surface it clearly.
  const settling = $derived(connected && busy);
  const canComply = $derived(can("comply"));
  const canTakeSlack = $derived(can("take_slack"));
  const canCalibrate = $derived(can("calibrate"));
  // Park is a motion move to a safe position; only meaningful once calibrated
  // and idle (READY_TO_CUT). `jog` is true only when FluidNC is Idle and no job
  // runs, so it also covers the job lock.
  const canPark = $derived(connected && policy?.code === 7 && (ap?.jog ?? false));
  // Diagnostic commands are anyState in the firmware; gate on a live link only.
  const canDiag = $derived(connected && (ap?.jog ?? false));

  // Short command names the firmware accepts (the embedded UI uses these;
  // the long `$Maslow/...` forms are rejected with error:3).
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

  // Park: lift Z to a safe height (work coords) then move to the park position
  // in machine coords. Mirrors the embedded UI's READY_TO_CUT park sequence,
  // using the configured park offsets (defaults if config not loaded).
  async function park() {
    const c = $maslowConfig;
    const z = c?.park_z ?? 2.0;
    const x = c?.park_x ?? 0.0;
    const y = c?.park_y ?? 0.0;
    await invoke("send_line", { line: `G90 G0 Z${z}` });
    await invoke("send_line", { line: `G53 G0 Y${y} X${x}` });
  }

  let showDiag = $state(false);
  function diag(cmd: "test" | "setZStop" | "calReset", confirmMsg?: string) {
    if (confirmMsg && !window.confirm(confirmMsg)) return;
    action(cmd);
  }

  function belt(label: string, len: number, err: number) {
    return { label, len, err };
  }
  const belts = $derived(
    info
      ? [
          belt("TL", info.tl, info.etl),
          belt("TR", info.tr, info.etr),
          belt("BL", info.bl, info.ebl),
          belt("BR", info.br, info.ebr),
        ]
      : [],
  );

  function fmt(n: number): string {
    return n.toFixed(1);
  }
</script>

<section class="maslow">
  <header>
    <span>Maslow</span>
    <span class="state" class:busy class:ready={policy?.code === 7}>
      {policy?.label ?? "—"}
    </span>
    {#if info}
      <span class="flags">
        <span class:on={calibrated}>calibré</span>
        <span class:on={info.homed}>homed</span>
        <span class:on={info.extended}>extended</span>
        {#if info.calibrationInProgress}<span class="on cal">calibrating</span>{/if}
      </span>
    {/if}
  </header>

  {#if belts.length > 0}
    <div class="belts">
      {#each belts as b}
        <div class="belt">
          <span class="bl">{b.label}</span>
          <span class="len">{fmt(b.len)}<small>mm</small></span>
          <span class="err" class:warn={Math.abs(b.err) > 1}>
            {b.err >= 0 ? "+" : ""}{fmt(b.err)}
          </span>
        </div>
      {/each}
    </div>
  {:else}
    <div class="hint">
      {connected ? "waiting for telemetry…" : "connect to view belts"}
    </div>
  {/if}

  {#if settling}
    <div class="settling">
      Machine en cours de stabilisation… <strong>Retract</strong> ramène à un état
      connu si elle reste bloquée.
    </div>
  {/if}

  <div class="actions">
    <button
      class:recover={settling && canRetract}
      onclick={() => action("retract")}
      disabled={!canRetract}>Retract</button
    >
    <button onclick={() => action("extend")} disabled={!canExtend}>Extend</button>
    <button onclick={() => action("comply")} disabled={!canComply}>Comply</button>
    <button onclick={() => action("takeSlack")} disabled={!canTakeSlack}>Take Slack</button>
    <button class="go" onclick={() => action("calibrate")} disabled={!canCalibrate}>
      Calibrate
    </button>
    <button onclick={park} disabled={!canPark} title="Lift Z and move to the configured park position">
      Park
    </button>
  </div>

  <div class="actions stop-row">
    <button class="warn" onclick={() => action("stop")} disabled={!can("stop")}>Stop</button>
    <button class="danger" onclick={() => action("estop")} disabled={!can("estop")}>E-Stop</button>
  </div>

  <details class="diag" bind:open={showDiag}>
    <summary>Diagnostics</summary>
    <div class="actions diag-row">
      <button
        onclick={() => diag("test", "Run the motor/sensor self-test? The machine will move.")}
        disabled={!canDiag}
        title="Run the Maslow self-test ($TEST)">Test</button
      >
      <button
        onclick={() => diag("setZStop")}
        disabled={!canDiag}
        title="Set the current Z position as the Z stop ($SETZSTOP)">Set Z Stop</button
      >
      <button
        class="warn"
        onclick={() =>
          diag("calReset", "Reset the calibration state machine? Use this to recover a stuck calibration.")}
        disabled={!canDiag}
        title="Reset the calibration state ($CALRESET)">Reset Calibration</button
      >
    </div>
  </details>
</section>

<style>
  .maslow {
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
    align-items: center;
    gap: 0.7em;
    font-size: 0.85em;
  }
  header > span:first-child {
    opacity: 0.85;
  }
  .state {
    font-weight: 700;
    padding: 0.15em 0.7em;
    border-radius: 6px;
    background: #2b3a5c;
  }
  .state.busy {
    background: #b8860b;
    animation: pulse 1.4s ease-in-out infinite;
  }
  .state.ready {
    background: #2e7d32;
  }
  @keyframes pulse {
    50% {
      opacity: 0.55;
    }
  }
  .flags {
    margin-left: auto;
    display: flex;
    gap: 0.5em;
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
    color: #e0a83d;
  }
  .belts {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 6px;
  }
  .belt {
    display: flex;
    align-items: baseline;
    gap: 0.5em;
    background: #1f1f1f;
    border-radius: 7px;
    padding: 0.4em 0.6em;
  }
  .bl {
    font-size: 0.75em;
    opacity: 0.55;
    width: 22px;
  }
  .len {
    font-size: 1.05em;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .len small {
    font-size: 0.6em;
    opacity: 0.5;
    margin-left: 2px;
  }
  .err {
    margin-left: auto;
    font-size: 0.78em;
    opacity: 0.55;
    font-variant-numeric: tabular-nums;
  }
  .err.warn {
    color: #ff6b6b;
    opacity: 1;
  }
  .hint {
    font-size: 0.8em;
    opacity: 0.5;
    padding: 0.3em 0;
  }
  .settling {
    font-size: 0.78em;
    line-height: 1.35;
    color: #e0a83d;
    background: #2a2008;
    border: 1px solid #6b4a1f;
    border-radius: 7px;
    padding: 0.4em 0.6em;
  }
  .settling strong {
    color: #ffd166;
  }
  .actions {
    display: flex;
    gap: 0.45em;
    flex-wrap: wrap;
  }
  button.recover {
    background: #2e7d32;
    border-color: #3ddc84;
    box-shadow: 0 0 0 1px #3ddc84 inset;
  }
  button {
    flex: 1;
    min-width: 72px;
    padding: 0.45em 0.6em;
    border-radius: 8px;
    border: 1px solid #555;
    background: #2b2b2b;
    color: #fff;
    cursor: pointer;
    font-size: 0.82em;
  }
  button:disabled {
    opacity: 0.35;
    cursor: not-allowed;
  }
  button.go {
    background: #2e7d32;
    border-color: #2e7d32;
  }
  .stop-row button.warn {
    background: #b8860b;
    border-color: #b8860b;
  }
  .stop-row button.danger {
    background: #8b2e2e;
    border-color: #8b2e2e;
  }
  .diag {
    margin-top: 0.5em;
    border-top: 1px solid #2a2a2a;
    padding-top: 0.4em;
  }
  .diag summary {
    cursor: pointer;
    color: #9a9a9a;
    font-size: 0.85em;
    user-select: none;
  }
  .diag-row {
    margin-top: 0.4em;
  }
  .diag-row button.warn {
    background: #b8860b;
    border-color: #b8860b;
  }
</style>
