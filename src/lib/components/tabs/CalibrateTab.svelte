<script lang="ts">
  import { waypoints, measurements, maslowState } from "$lib/stores/maslow";
  import CalibrationWizard from "$lib/components/CalibrationWizard.svelte";
  import CalibrationView from "$lib/components/CalibrationView.svelte";
  import CalibrationSolver from "$lib/components/CalibrationSolver.svelte";
  import BeltControls from "$lib/components/controls/BeltControls.svelte";

  // The map + solver only have content during/after a run; show them only then.
  const showMap = $derived($waypoints.length > 0 || ($maslowState?.busy ?? false));
  const showSolver = $derived($measurements.length > 0);
</script>

<div class="cal-tab">
  <section class="card">
    <h3>Guided calibration</h3>
    <CalibrationWizard />
  </section>

  <!-- The wizard is the primary path; manual belt control is collapsed by default
       (it's also on Main → Belts). -->
  <details class="card acc">
    <summary>Belts — manual control</summary>
    <div class="acc-body"><BeltControls /></div>
  </details>

  {#if showMap}
    <details class="card acc" open>
      <summary>Waypoint map</summary>
      <div class="acc-body"><CalibrationView /></div>
    </details>
  {/if}
  {#if showSolver}
    <details class="card acc" open>
      <summary>Anchor solver</summary>
      <div class="acc-body"><CalibrationSolver /></div>
    </details>
  {/if}
</div>

<style>
  .cal-tab {
    display: flex;
    flex-direction: column;
    gap: var(--gap);
    padding: var(--gap);
  }
  .card {
    background: var(--surface);
    border: 1px solid var(--border-2);
    border-radius: var(--radius-lg);
    padding: var(--gap-lg);
  }
  h3 {
    margin: 0 0 var(--gap) 0;
    font-size: 0.9em;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-dim);
  }
  .acc {
    padding: var(--gap);
  }
  .acc > summary {
    cursor: pointer;
    color: var(--text-dim);
    font-size: 0.9em;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    user-select: none;
  }
  .acc[open] > summary {
    margin-bottom: var(--gap);
  }
</style>
