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

<!-- Each component owns its own card; no extra wrapper boxes. The wizard is the
     primary path, manual belt control is collapsed (also on Main → Belts), and the
     map/solver appear only when a run produces data. -->
<div class="cal-tab">
  <CalibrationWizard />

  <details class="belts-acc">
    <summary>Belts — manual control</summary>
    <div class="belts-body"><BeltControls /></div>
  </details>

  {#if showMap}<CalibrationView />{/if}
  {#if showSolver}<CalibrationSolver />{/if}
</div>

<style>
  .cal-tab {
    display: flex;
    flex-direction: column;
    gap: var(--gap);
    padding: var(--gap);
  }
  /* The belt controls have no card of their own, so the accordion provides it. */
  .belts-acc {
    background: var(--surface);
    border: 1px solid var(--border-2);
    border-radius: var(--radius-lg);
    padding: var(--gap);
  }
  .belts-acc > summary {
    cursor: pointer;
    color: var(--text-dim);
    font-size: 0.9em;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    user-select: none;
  }
  .belts-acc[open] > summary {
    margin-bottom: var(--gap);
  }
</style>
