<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { wsState } from "$lib/stores/machine";
  import {
    maslowState,
    actionPolicy,
    waypoints,
    calComplete,
    anchors,
  } from "$lib/stores/maslow";
  import { CalState, isReadyToCut, isResumablePreCut } from "$lib/stores/calState";
  import { connection, fwVersion } from "$lib/stores/connection";
  import { supportsFullCalibration } from "$lib/stores/firmware";

  // Guided layer on top of the contextual MaslowPanel. The firmware owns the
  // calibration state machine, so prerequisites/enablement come straight from
  // the unified action policy (no gating rules duplicated here) and the live
  // Maslow state drives which step is active.

  type PolicyKey =
    | "retract"
    | "extend"
    | "take_slack"
    | "calibrate"
    | "comply";

  interface Step {
    key: string;
    title: string;
    /** Short button label when the full title would be redundant. */
    btn?: string;
    hint: string;
    cmd: string | null;
    policy: PolicyKey | null;
    /** Calibration state code reported while this step is running. */
    busyCode: number | null;
    optional?: boolean;
    /** Operator action with no firmware command (e.g. lower Z by hand). */
    manual?: boolean;
  }

  // Sequence follows the firmware transition matrix, with a manual Z-lowering
  // step inserted before extend (the belts lock the axes once extending, so Z
  // must be set down while it can still move):
  // retract → lower Z → extend → take slack → calibrate → computing → ready.
  const STEPS: Step[] = [
    {
      key: "retract",
      title: "Retract all belts",
      hint: "Pull all four belts fully in to establish a known zero.",
      cmd: "$ALL",
      policy: "retract",
      busyCode: 1,
    },
    {
      key: "lowerZ",
      title: "Lower Z all the way down",
      hint: "Jog Z down until the bit (or spindle nose) just touches the spoilboard. This can't be automatic — it depends on whether a bit is installed. Do it now: once the belts extend the axes are locked and Z can't move.",
      cmd: null,
      policy: null,
      busyCode: null,
      manual: true,
    },
    {
      key: "extend",
      title: "Extend belts to maximum",
      btn: "Extend",
      hint: "Let all four belts pay all the way out and stop on their own — do NOT press Stop. Calibration can't start until every belt reaches maximum and the state reads “Belts Extended”.",
      cmd: "$EXT",
      policy: "extend",
      busyCode: 3,
    },
    {
      key: "takeSlack",
      title: "Take slack",
      hint: "Tension the belts before measuring (recommended).",
      cmd: "$TKSLK",
      policy: "take_slack",
      busyCode: 5,
      optional: true,
    },
    {
      key: "calibrate",
      title: "Calibrate",
      hint: "Run the measurement grid — the machine drives to each waypoint.",
      cmd: "$CAL",
      policy: "calibrate",
      busyCode: 6,
    },
    {
      key: "computing",
      title: "Computing",
      hint: "Solving the anchor geometry from the measurements.",
      cmd: null,
      policy: null,
      busyCode: 9,
    },
    {
      key: "ready",
      title: "Ready to cut",
      hint: "Calibration complete — the machine is ready.",
      cmd: null,
      policy: null,
      busyCode: null,
    },
  ];

  // Client-side acknowledgement that the operator lowered Z (the firmware does
  // not track this). Gates Extend so the Z step can't be skipped.
  let zLowered = $state(false);

  const connected = $derived($wsState === "connected");
  const ap = $derived($actionPolicy);
  const mState = $derived($maslowState);
  const code = $derived(mState?.code ?? null);
  const busy = $derived(mState?.busy ?? false);

  // Calibration status from the firmware config (anchors persisted in
  // maslow.yaml, reloaded at boot). `valid` means the machine already knows its
  // geometry → no recalibration needed, just re-tension.
  const calib = $derived($anchors);
  const calibrated = $derived(calib?.calibrated ?? false);

  // This app doesn't implement the v1.21 `$ACKCAL` recompute handshake, so Calibrate
  // is gated off below that firmware version. Unknown versions aren't punished.
  const calibrationSupported = $derived(supportsFullCalibration($fwVersion));

  // Daily "resume" path: the machine booted with calibration intact and is in a
  // stable pre-cut state. From EXTENDEDOUT(4) we can apply tension straight to
  // READY_TO_CUT; from RETRACTED(2) we extend first, then apply tension.
  const resumeable = $derived(
    connected && calibrated && !busy && isResumablePreCut(code),
  );

  // The single next action that walks toward Ready to Cut on the resume path.
  type ResumeAction = { label: string; hint: string; cmd: string } | null;
  const resumeAction: ResumeAction = $derived(
    !resumeable
      ? null
      : ap?.take_slack
        ? {
            label: "Resume — apply tension",
            hint: "Tensions the belts and moves to Ready to Cut. No recalibration needed.",
            cmd: "$TKSLK",
          }
        : ap?.extend
          ? {
              label: "Resume — extend belts",
              hint: "Pays the belts out, then apply tension to reach Ready to Cut.",
              cmd: "$EXT",
            }
          : null,
  );

  // The full first-time/recovery sequence is collapsed by default once a resume
  // is on offer, but always reachable via the toggle.
  let showFull = $state(false);
  const fullVisible = $derived(showFull || !resumeable);

  // Map the live calibration state code onto the active wizard step. The
  // firmware owns the state, so this advances on its own as reports arrive.
  function stepForCode(c: number | null): number {
    switch (c) {
      case CalState.Retracting:
        return 0;
      case CalState.Retracted:
        return 1; // retracted → lower Z next
      case CalState.Extending:
        return 2;
      case CalState.ExtendedOut:
        return 3; // extended → take slack / calibrate
      case CalState.TakingSlack:
        return 3;
      case CalState.CalibrationInProgress:
        return 4;
      case CalState.CalibrationComputing:
        return 5;
      case CalState.ReadyToCut:
        return 6;
      default:
        return 0; // unknown / releasing tension → start over
    }
  }

  const current = $derived(stepForCode(code));

  function status(i: number): "done" | "busy" | "active" | "pending" {
    if (i < current) return "done";
    if (i > current) return "pending";
    // Current step: spinning if the firmware reports its busy code.
    return busy && STEPS[i].busyCode === code ? "busy" : "active";
  }

  // Action enablement is the policy's job; we never re-derive firmware
  // prerequisites — we only add the client-side Z acknowledgement on Extend.
  function canDo(s: Step): boolean {
    if (!connected || s.policy == null || !(ap?.[s.policy] ?? false)) return false;
    if (s.key === "extend" && !zLowered) return false;
    if (s.key === "calibrate" && !calibrationSupported) return false;
    return true;
  }

  // Plain-language reason an enabled-looking step is still blocked, so the user
  // is never left staring at a dead button (the firmware error loop is already
  // prevented by the policy gating; this removes the confusion behind it).
  function disabledReason(s: Step): string | null {
    if (!connected) return "Not connected.";
    if (s.key === "extend" && !zLowered)
      return "Confirm Z is lowered all the way down first.";
    if (s.key === "calibrate" && !calibrationSupported)
      return `Full calibration on firmware v${$fwVersion} requires the embedded web UI at http://${$connection.host}. This app supports calibration from firmware v1.22 onward, everything else in this app works normally.`;
    if (canDo(s)) return null;
    if (s.key === "takeSlack" || s.key === "calibrate") {
      if (code === CalState.Extending)
        return "Belts still extending — wait until they reach MAXIMUM (state “Belts Extended”).";
      if (code === CalState.Unknown || code === CalState.Retracted)
        return "Belts aren't extended to maximum. Retract → lower Z → Extend fully before calibrating.";
      if (busy) return "Machine busy — let the current step finish.";
    }
    return null;
  }

  function run(cmd: string | null) {
    if (!cmd) return;
    // Calibrate drives the full measurement grid; confirm before it moves.
    if (
      cmd === "$CAL" &&
      !window.confirm(
        "Start calibration? The machine will drive to every measurement waypoint across the work area.",
      )
    )
      return;
    invoke("send_line", { line: cmd });
  }
</script>

<section class="wizard">
  <header>
    <span>Calibration Wizard</span>
    <span class="state" class:busy class:ready={isReadyToCut(code)}>
      {mState?.label ?? "—"}
    </span>
    {#if code === CalState.CalibrationInProgress}
      <span class="wp">{$waypoints.length} waypoints</span>
    {/if}
  </header>

  {#if calib}
    <div class="cal-badge" class:ok={calibrated}>
      {#if calibrated}
        Calibrated ✓ <small>(anchors in memory)</small>
      {:else}
        Not calibrated <small>— calibration required</small>
      {/if}
    </div>
  {/if}

  <!-- Calibrated but in UNKNOWN: the anchor geometry survives a reboot (it
       lives in maslow.yaml), but belt-length tracking does not unless the last
       stop saved it. A hard E-STOP / power-cycle loses it, so the firmware
       genuinely needs Retract → Extend to re-zero the belts. Spell that out so
       "Calibrated ✓ but must retract?!" isn't a contradiction. -->
  {#if calibrated && code === CalState.Unknown}
    <div class="lost-belts">
      Calibration is kept, but the belt lengths were lost (hard stop or
      power-cycle). <strong>Retract → Extend</strong> to re-zero the belts, then
      apply tension. Next time use <strong>Stop</strong> (not E-Stop): it saves
      the belt positions so you can resume with just <strong>Apply Tension</strong>.
    </div>
  {/if}

  {#if $calComplete}
    <div class="done-banner">✓ Calibration complete — ready to cut</div>
  {/if}

  {#if resumeAction}
    <div class="resume">
      <div class="resume-head">Resume</div>
      <p class="resume-hint">{resumeAction.hint}</p>
      <button class="resume-go" onclick={() => run(resumeAction.cmd)}>
        {resumeAction.label}
      </button>
    </div>
  {/if}

  <!-- At Ready to Cut, let the operator relax the belts for the night so the
       belts + frame don't sit under tension. $CMP runs the firmware's
       release-tension transition; the morning resume is Retract → Extend →
       Apply Tension (calibration stays valid). -->
  {#if isReadyToCut(code)}
    <div class="release">
      <div class="release-head">Done for the day?</div>
      <p class="release-hint">
        Release the belt tension so the belts and frame rest overnight. In the
        morning, run <strong>Retract → Extend → Apply Tension</strong> to resume —
        the calibration stays valid.
      </p>
      <button
        class="release-go"
        onclick={() => run("$CMP")}
        disabled={!(ap?.comply ?? false)}
      >
        Release tension
      </button>
    </div>
  {/if}

  {#if resumeable}
    <button class="toggle" onclick={() => (showFull = !showFull)}>
      {fullVisible
        ? "Hide full sequence"
        : "Full calibration (first time / recovery)"}
    </button>
  {/if}

  {#if fullVisible}
  <ol class="steps">
    {#each STEPS as s, i (s.key)}
      {@const st = status(i)}
      <li class="step {st}">
        <span class="marker">
          {#if st === "done"}✓{:else if st === "busy"}●{:else}{i + 1}{/if}
        </span>
        <div class="body">
          <div class="title">
            {s.title}
            {#if s.optional}<small class="opt">optional</small>{/if}
          </div>
          {#if st === "active" || st === "busy"}
            <div class="hint">{s.hint}</div>
            {#if s.cmd && disabledReason(s)}
              <div class="reason">{disabledReason(s)}</div>
            {/if}
          {/if}
        </div>
        {#if s.manual}
          <label class="ack" class:checked={zLowered}>
            <input type="checkbox" bind:checked={zLowered} />
            Z is down
          </label>
        {:else if s.cmd}
          <button
            class:go={s.key === "calibrate"}
            onclick={() => run(s.cmd)}
            disabled={!canDo(s)}
          >
            {st === "busy" ? "Running…" : (s.btn ?? s.title)}
          </button>
        {/if}
      </li>
    {/each}
  </ol>
  {/if}

  {#if busy}
    <div class="settling-hint">
      If stuck after a Stop, use <strong>Retract</strong> (Maslow panel) — it's the
      only action the firmware accepts from a transitional state.
    </div>
  {/if}

  {#if !connected}
    <div class="offline">Connect to start the calibration workflow.</div>
  {/if}
</section>

<style>
  .wizard {
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
  .wp {
    margin-left: auto;
    font-size: 0.78em;
    color: #7fb2ff;
  }
  @keyframes pulse {
    50% {
      opacity: 0.55;
    }
  }
  .steps {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.4em;
  }
  .step {
    display: flex;
    align-items: center;
    gap: 0.6em;
    padding: 0.45em 0.55em;
    border-radius: 8px;
    background: #1c1c1c;
    border: 1px solid #2a2a2a;
  }
  .step.active {
    border-color: #396cd8;
  }
  .step.busy {
    border-color: #b8860b;
  }
  .step.pending {
    opacity: 0.5;
  }
  .marker {
    flex: 0 0 22px;
    height: 22px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    font-size: 0.78em;
    font-weight: 700;
    background: #333;
    color: #ddd;
  }
  .step.done .marker {
    background: #2e7d32;
  }
  .step.active .marker {
    background: #396cd8;
  }
  .step.busy .marker {
    background: #b8860b;
    animation: pulse 1.4s ease-in-out infinite;
  }
  .body {
    flex: 1;
    min-width: 0;
  }
  .title {
    font-size: 0.88em;
    font-weight: 600;
  }
  .opt {
    font-weight: 400;
    opacity: 0.5;
    margin-left: 0.4em;
    font-size: 0.85em;
  }
  .hint {
    font-size: 0.76em;
    opacity: 0.6;
    margin-top: 0.15em;
  }
  .reason {
    font-size: 0.75em;
    margin-top: 0.3em;
    color: #e0a83d;
  }
  .ack {
    display: flex;
    align-items: center;
    gap: 0.35em;
    font-size: 0.78em;
    white-space: nowrap;
    cursor: pointer;
    padding: 0.4em 0.6em;
    border: 1px solid #555;
    border-radius: 8px;
    background: #2b2b2b;
  }
  .ack.checked {
    border-color: #2e7d32;
    background: #14301f;
    color: #3ddc84;
  }
  .ack input {
    cursor: pointer;
  }
  button {
    padding: 0.4em 0.7em;
    border-radius: 8px;
    border: 1px solid #555;
    background: #2b2b2b;
    color: #fff;
    cursor: pointer;
    font-size: 0.8em;
    white-space: nowrap;
  }
  button:disabled {
    opacity: 0.35;
    cursor: not-allowed;
  }
  button.go {
    background: #2e7d32;
    border-color: #2e7d32;
  }
  .done-banner {
    font-size: 0.85em;
    color: #3ddc84;
    background: #14301f;
    border: 1px solid #1f5a36;
    border-radius: 7px;
    padding: 0.4em 0.6em;
  }
  .cal-badge {
    font-size: 0.82em;
    font-weight: 600;
    padding: 0.35em 0.6em;
    border-radius: 7px;
    background: #3a2a14;
    border: 1px solid #6b4a1f;
    color: #e0a83d;
  }
  .cal-badge.ok {
    background: #14301f;
    border-color: #1f5a36;
    color: #3ddc84;
  }
  .cal-badge small {
    font-weight: 400;
    opacity: 0.8;
  }
  .lost-belts {
    font-size: 0.78em;
    line-height: 1.4;
    color: #e0a83d;
    background: #2a2008;
    border: 1px solid #6b4a1f;
    border-radius: 7px;
    padding: 0.45em 0.6em;
  }
  .lost-belts strong {
    color: #ffd166;
  }
  .resume {
    background: #14223a;
    border: 1px solid #2f5a9c;
    border-radius: 9px;
    padding: 0.6em 0.7em;
    display: flex;
    flex-direction: column;
    gap: 0.45em;
  }
  .resume-head {
    font-size: 0.72em;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    opacity: 0.7;
  }
  .resume-hint {
    margin: 0;
    font-size: 0.8em;
    opacity: 0.8;
    line-height: 1.35;
  }
  .resume-go {
    background: #2e7d32;
    border-color: #2e7d32;
    font-size: 0.9em;
    font-weight: 600;
    padding: 0.55em 0.8em;
  }
  .release {
    background: #241a08;
    border: 1px solid #6b4a1f;
    border-radius: 9px;
    padding: 0.6em 0.7em;
    display: flex;
    flex-direction: column;
    gap: 0.45em;
  }
  .release-head {
    font-size: 0.72em;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    opacity: 0.7;
  }
  .release-hint {
    margin: 0;
    font-size: 0.8em;
    opacity: 0.8;
    line-height: 1.35;
  }
  .release-hint strong {
    color: #ffd166;
  }
  .release-go {
    background: #b8860b;
    border-color: #b8860b;
    font-size: 0.9em;
    font-weight: 600;
    padding: 0.55em 0.8em;
    align-self: flex-start;
  }
  .release-go:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
  .toggle {
    background: transparent;
    border: 1px dashed #555;
    color: #9bb4d8;
    font-size: 0.78em;
    align-self: flex-start;
  }
  .settling-hint {
    font-size: 0.76em;
    line-height: 1.35;
    color: #e0a83d;
    background: #2a2008;
    border: 1px solid #6b4a1f;
    border-radius: 7px;
    padding: 0.4em 0.6em;
  }
  .settling-hint strong {
    color: #ffd166;
  }
  .offline {
    font-size: 0.8em;
    opacity: 0.5;
  }
</style>
