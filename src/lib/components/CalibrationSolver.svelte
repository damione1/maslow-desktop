<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { wsState } from "$lib/stores/machine";
  import { connection } from "$lib/stores/connection";
  import { jobProgress } from "$lib/stores/job";
  import {
    measurements,
    firmwareFit,
    firmwareAnchors,
    localSolve,
    excludedWaypoints,
    anchors,
    solveLocally,
    toggleExcluded,
    refreshAnchors,
  } from "$lib/stores/maslow";

  const connected = $derived($wsState === "connected");
  const jobActive = $derived(
    $jobProgress?.state === "running" || $jobProgress?.state === "paused",
  );

  // Single-waypoint belt error above which a point is flagged as suspect, mm.
  // Mirrors the firmware's per-residual fail gate.
  const OUTLIER_MM = 15;

  let busy = $state(false);
  let message = $state("");
  let error = $state("");

  // Per-original-index max |residual| from the last solve, for the table.
  const maxResByIndex = $derived.by(() => {
    const out = new Map<number, number>();
    const solve = $localSolve;
    if (!solve) return out;
    solve.kept_indices.forEach((orig, i) => {
      const r = solve.residuals[i];
      out.set(orig, Math.max(...r.map(Math.abs)));
    });
    return out;
  });

  const beltLabels = ["TL", "TR", "BL", "BR"] as const;

  async function solve() {
    busy = true;
    error = "";
    message = "";
    try {
      await solveLocally();
    } catch (e) {
      error = `Solve failed: ${e}`;
    } finally {
      busy = false;
    }
  }

  function onToggle(i: number) {
    toggleExcluded(i);
    // Stale once the exclusion set changes — force an explicit re-solve.
    localSolve.set(null);
  }

  const canWrite = $derived(
    connected &&
      !jobActive &&
      !busy &&
      ($localSolve?.ok ?? false) &&
      ($localSolve?.anchors.valid ?? false),
  );

  // Write the 5 reduced anchor params the firmware itself sets after its LM.
  const ANCHOR_PATHS: [keyof NonNullable<typeof $localSolve>["params"], string][] =
    [
      ["tl_x", "kinematics/MaslowKinematics/tlX"],
      ["tl_y", "kinematics/MaslowKinematics/tlY"],
      ["tr_x", "kinematics/MaslowKinematics/trX"],
      ["tr_y", "kinematics/MaslowKinematics/trY"],
      ["br_x", "kinematics/MaslowKinematics/brX"],
    ];

  async function writeAnchors() {
    const solve = $localSolve;
    if (!solve) return;
    busy = true;
    error = "";
    message = "";
    const host = $connection.host;
    try {
      for (const [key, path] of ANCHOR_PATHS) {
        await invoke("write_maslow_setting", {
          host,
          path,
          value: String(solve.params[key]),
        });
      }
      await invoke("save_maslow_config", { host });
      message = "Anchors written to machine flash.";
      await refreshAnchors();
    } catch (e) {
      error = `Write failed: ${e}`;
    } finally {
      busy = false;
    }
  }

  const fmt = (n: number) => n.toFixed(1);
</script>

<section class="solver">
  <header>
    <span>Local solver</span>
    <span class="sub">Levenberg–Marquardt</span>
    {#if $measurements.length}
      <span class="count">{$measurements.length} waypoints</span>
    {/if}
  </header>

  {#if !$measurements.length}
    <div class="hint">
      Raw measurements appear here after a calibration recompute (the firmware
      logs <code>CLBM:</code>). Then you can re-solve anchors locally, exclude a
      suspect waypoint without re-measuring, and write the result.
    </div>
  {:else}
    <div class="actions">
      <button class="primary" onclick={solve} disabled={busy}>
        {busy ? "Solving…" : "Solve locally"}
      </button>
      {#if $excludedWaypoints.size}
        <span class="excluded">{$excludedWaypoints.size} excluded</span>
      {/if}
    </div>

    <table>
      <thead>
        <tr>
          <th>#</th>
          <th>TL</th>
          <th>TR</th>
          <th>BL</th>
          <th>BR</th>
          <th>max|r|</th>
          <th>use</th>
        </tr>
      </thead>
      <tbody>
        {#each $measurements as m, i}
          {@const mr = maxResByIndex.get(i)}
          {@const excluded = $excludedWaypoints.has(i)}
          {@const outlier = mr !== undefined && mr > OUTLIER_MM}
          <tr class:excluded class:outlier>
            <td>{i}</td>
            <td>{fmt(m.tl)}</td>
            <td>{fmt(m.tr)}</td>
            <td>{fmt(m.bl)}</td>
            <td>{fmt(m.br)}</td>
            <td class="res">{mr === undefined ? "—" : mr.toFixed(2)}</td>
            <td>
              <input
                type="checkbox"
                checked={!excluded}
                onchange={() => onToggle(i)}
                title={excluded ? "excluded from solve" : "included in solve"}
              />
            </td>
          </tr>
        {/each}
      </tbody>
    </table>

    {#if $localSolve}
      {@const s = $localSolve}
      <div class="fit" class:fail={!s.ok}>
        <div class="row">
          <span class="k">Local fit</span>
          <span class="v">
            rms {s.fitness.rms.toFixed(2)} mm · max {s.fitness.max_residual.toFixed(
              2,
            )} mm · {s.fitness.converged ? "converged" : "stalled"}
          </span>
        </div>
        <div class="row per">
          <span class="k">per-anchor rms</span>
          <span class="v">
            {#each s.fitness.per_anchor as p, j}
              <span>{beltLabels[j]} {p.toFixed(2)}</span>
            {/each}
          </span>
        </div>
        {#if !s.ok}
          <div class="gate">✗ gates failed: {s.gate_error}</div>
        {:else}
          <div class="gate ok">✓ passes firmware gates</div>
        {/if}

        <div class="anchors">
          <div class="ac head">
            <span></span><span>local</span><span>firmware</span>
          </div>
          {#each [["tl_x", "tlX"], ["tl_y", "tlY"], ["tr_x", "trX"], ["tr_y", "trY"], ["br_x", "brX"]] as [key, label]}
            <div class="ac">
              <span class="lab">{label}</span>
              <span>{s.params[key as keyof typeof s.params].toFixed(1)}</span>
              <span class="fw">
                {$firmwareAnchors
                  ? $firmwareAnchors[
                      key as keyof NonNullable<typeof $firmwareAnchors>
                    ].toFixed(1)
                  : "—"}
              </span>
            </div>
          {/each}
        </div>

        {#if $firmwareFit}
          <div class="row fwfit">
            <span class="k">firmware fit</span>
            <span class="v">
              rms {$firmwareFit.rms.toFixed(2)} mm · max {$firmwareFit.max_residual.toFixed(
                2,
              )} mm
            </span>
          </div>
        {/if}

        <button class="write" onclick={writeAnchors} disabled={!canWrite}>
          Write these anchors
        </button>
        {#if !canWrite && s.ok}
          <div class="why">
            {jobActive
              ? "blocked while a job is running"
              : !connected
                ? "connect to write"
                : !s.anchors.valid
                  ? "geometry fails sanity check"
                  : ""}
          </div>
        {/if}
      </div>
    {/if}

    {#if message}<div class="msg ok">{message}</div>{/if}
    {#if error}<div class="msg err">{error}</div>{/if}
  {/if}
</section>

<style>
  .solver {
    border: 1px solid #2a2a2a;
    border-radius: 8px;
    background: #1a1a1a;
    padding: 0.75em 0.9em 0.9em;
    display: flex;
    flex-direction: column;
    gap: 0.6em;
    font-size: 0.85em;
  }
  header {
    display: flex;
    align-items: baseline;
    gap: 0.6em;
  }
  header > span:first-child {
    font-weight: 600;
  }
  .sub {
    color: #888;
    font-size: 0.85em;
  }
  .count {
    margin-left: auto;
    color: #9a9a9a;
  }
  .hint {
    color: #9a9a9a;
    line-height: 1.4;
  }
  .hint code {
    background: #262626;
    padding: 0 0.3em;
    border-radius: 3px;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 0.7em;
  }
  .excluded {
    color: #d8a657;
  }
  button.primary {
    background: #2f6f4f;
    color: #fff;
    border: none;
    border-radius: 5px;
    padding: 0.4em 0.9em;
    cursor: pointer;
  }
  button.primary:disabled {
    opacity: 0.5;
    cursor: default;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-variant-numeric: tabular-nums;
  }
  th,
  td {
    text-align: right;
    padding: 0.18em 0.4em;
  }
  th:first-child,
  td:first-child,
  th:last-child,
  td:last-child {
    text-align: center;
  }
  thead th {
    color: #888;
    font-weight: 500;
    border-bottom: 1px solid #2a2a2a;
  }
  tbody tr.outlier {
    color: #ea6962;
  }
  tbody tr.excluded {
    opacity: 0.4;
    text-decoration: line-through;
  }
  td.res {
    color: #9a9a9a;
  }
  .fit {
    border-top: 1px solid #2a2a2a;
    padding-top: 0.6em;
    display: flex;
    flex-direction: column;
    gap: 0.4em;
  }
  .fit.fail .gate {
    color: #ea6962;
  }
  .row {
    display: flex;
    gap: 0.6em;
  }
  .row .k {
    color: #888;
    min-width: 9em;
  }
  .per .v {
    display: flex;
    gap: 0.7em;
    flex-wrap: wrap;
    color: #b0b0b0;
  }
  .gate.ok {
    color: #a9b665;
  }
  .anchors {
    display: flex;
    flex-direction: column;
    gap: 0.15em;
    font-variant-numeric: tabular-nums;
  }
  .ac {
    display: grid;
    grid-template-columns: 3em 1fr 1fr;
    gap: 0.5em;
  }
  .ac span:not(.lab):not(.fw) {
    text-align: right;
  }
  .ac .fw {
    text-align: right;
    color: #9a9a9a;
  }
  .ac.head {
    color: #888;
  }
  .ac.head span {
    text-align: right;
  }
  .lab {
    color: #888;
  }
  .fwfit .v {
    color: #9a9a9a;
  }
  button.write {
    align-self: flex-start;
    margin-top: 0.3em;
    background: #45403d;
    color: #fff;
    border: 1px solid #5a5a5a;
    border-radius: 5px;
    padding: 0.35em 0.8em;
    cursor: pointer;
  }
  button.write:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .why {
    color: #888;
    font-size: 0.9em;
  }
  .msg.ok {
    color: #a9b665;
  }
  .msg.err {
    color: #ea6962;
  }
</style>
