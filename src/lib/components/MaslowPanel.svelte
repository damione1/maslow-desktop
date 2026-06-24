<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { wsState } from "$lib/stores/machine";
  import { jobProgress } from "$lib/stores/job";
  import { maslowInfo, maslowState } from "$lib/stores/maslow";

  const connected = $derived($wsState === "connected");
  const jobActive = $derived(
    $jobProgress !== null &&
      $jobProgress.state !== "done" &&
      $jobProgress.state !== "error",
  );
  const policy = $derived($maslowState);
  const busy = $derived(policy?.busy ?? false);
  const info = $derived($maslowInfo);

  // Base gate: connected, not streaming a job.
  const ready = $derived(connected && !jobActive);

  // Allowed actions come straight from the Rust state machine (single source
  // of truth, derived from the firmware transition guards).
  const can = (action: string) =>
    ready && (policy?.allowed.includes(action) ?? false);
  const canRetract = $derived(can("retract"));
  const canExtend = $derived(can("extend"));
  const canComply = $derived(can("comply"));
  const canTakeSlack = $derived(can("takeSlack"));
  const canCalibrate = $derived(can("calibrate"));

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
  } as const;

  function action(cmd: keyof typeof CMD) {
    invoke("send_line", { line: CMD[cmd] });
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

  <div class="actions">
    <button onclick={() => action("retract")} disabled={!canRetract}>Retract</button>
    <button onclick={() => action("extend")} disabled={!canExtend}>Extend</button>
    <button onclick={() => action("comply")} disabled={!canComply}>Comply</button>
    <button onclick={() => action("takeSlack")} disabled={!canTakeSlack}>Take Slack</button>
    <button class="go" onclick={() => action("calibrate")} disabled={!canCalibrate}>
      Calibrate
    </button>
  </div>

  <div class="actions stop-row">
    <button class="warn" onclick={() => action("stop")} disabled={!can("stop")}>Stop</button>
    <button class="danger" onclick={() => action("estop")} disabled={!can("estop")}>E-Stop</button>
  </div>
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
  .actions {
    display: flex;
    gap: 0.45em;
    flex-wrap: wrap;
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
</style>
