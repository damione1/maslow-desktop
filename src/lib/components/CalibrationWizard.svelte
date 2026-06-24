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
    hint: string;
    cmd: string | null;
    policy: PolicyKey | null;
    /** Calibration state code reported while this step is running. */
    busyCode: number | null;
    optional?: boolean;
  }

  // Sequence follows the firmware transition matrix:
  // retract → extend → take slack → calibrate → computing → ready to cut.
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
      key: "extend",
      title: "Extend",
      hint: "Pay the belts back out so they can be hooked to the sled.",
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

  const connected = $derived($wsState === "connected");
  const ap = $derived($actionPolicy);
  const mState = $derived($maslowState);
  const code = $derived(mState?.code ?? null);
  const busy = $derived(mState?.busy ?? false);

  // Calibration status from the firmware config (anchors persisted in
  // maslow.yaml, reloaded at boot). `valid` means the machine already knows its
  // geometry → no recalibration needed, just re-tension.
  const calib = $derived($anchors);
  const calibrated = $derived(calib?.valid ?? false);

  // Daily "resume" path: the machine booted with calibration intact and is in a
  // stable pre-cut state. From EXTENDEDOUT(4) we can apply tension straight to
  // READY_TO_CUT; from RETRACTED(2) we extend first, then apply tension.
  const resumeable = $derived(
    connected && calibrated && !busy && (code === 4 || code === 2),
  );

  // The single next action that walks toward Ready to Cut on the resume path.
  type ResumeAction = { label: string; hint: string; cmd: string } | null;
  const resumeAction: ResumeAction = $derived(
    !resumeable
      ? null
      : ap?.take_slack
        ? {
            label: "Reprendre — appliquer la tension",
            hint: "Tend les courroies puis passe en Prêt à couper. Aucune recalibration nécessaire.",
            cmd: "$TKSLK",
          }
        : ap?.extend
          ? {
              label: "Reprendre — étendre les courroies",
              hint: "Déroule les courroies, puis applique la tension pour passer en Prêt à couper.",
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
      case 1:
        return 0; // retracting
      case 2:
        return 1; // retracted → extend next
      case 3:
        return 1; // extending
      case 4:
        return 2; // extended → take slack / calibrate
      case 5:
        return 2; // taking slack
      case 6:
        return 3; // calibrating
      case 9:
        return 4; // computing
      case 7:
        return 5; // ready to cut
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

  // Action enablement is the policy's job; we never re-derive prerequisites.
  function canDo(s: Step): boolean {
    return connected && s.policy != null && (ap?.[s.policy] ?? false);
  }

  function run(cmd: string | null) {
    if (cmd) invoke("send_line", { line: cmd });
  }

  const canStop = $derived(connected && (ap?.stop ?? false));
  const canEstop = $derived(connected && (ap?.estop ?? false));
</script>

<section class="wizard">
  <header>
    <span>Calibration Wizard</span>
    <span class="state" class:busy class:ready={code === 7}>
      {mState?.label ?? "—"}
    </span>
    {#if code === 6}
      <span class="wp">{$waypoints.length} waypoints</span>
    {/if}
  </header>

  {#if calib}
    <div class="cal-badge" class:ok={calibrated}>
      {#if calibrated}
        Calibré ✓ <small>(ancrages en mémoire)</small>
      {:else}
        Non calibré <small>— calibration requise</small>
      {/if}
    </div>
  {/if}

  {#if $calComplete}
    <div class="done-banner">✓ Calibration complete — ready to cut</div>
  {/if}

  {#if resumeAction}
    <div class="resume">
      <div class="resume-head">Reprise</div>
      <p class="resume-hint">{resumeAction.hint}</p>
      <button class="resume-go" onclick={() => run(resumeAction.cmd)}>
        {resumeAction.label}
      </button>
    </div>
  {/if}

  {#if resumeable}
    <button class="toggle" onclick={() => (showFull = !showFull)}>
      {fullVisible
        ? "Masquer la séquence complète"
        : "Calibration complète (première fois / récupération)"}
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
          {/if}
        </div>
        {#if s.cmd}
          <button
            class:go={s.key === "calibrate"}
            onclick={() => run(s.cmd)}
            disabled={!canDo(s)}
          >
            {st === "busy" ? "Running…" : s.title}
          </button>
        {/if}
      </li>
    {/each}
  </ol>
  {/if}

  {#if busy}
    <div class="settling-hint">
      En cas de blocage après un Stop, utilisez <strong>Retract</strong> (panneau
      Maslow) — c'est la seule action que le firmware accepte depuis un état
      transitoire.
    </div>
  {/if}

  <div class="stop-row">
    <button class="warn" onclick={() => run("$STOP")} disabled={!canStop}>
      Stop
    </button>
    <button class="danger" onclick={() => run("$ESTOP")} disabled={!canEstop}>
      E-Stop
    </button>
  </div>

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
  .stop-row {
    display: flex;
    gap: 0.45em;
  }
  .stop-row button {
    flex: 1;
  }
  .stop-row .warn {
    background: #b8860b;
    border-color: #b8860b;
  }
  .stop-row .danger {
    background: #8b2e2e;
    border-color: #8b2e2e;
  }
  .offline {
    font-size: 0.8em;
    opacity: 0.5;
  }
</style>
