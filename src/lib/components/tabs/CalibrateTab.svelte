<script lang="ts">
  import { layout } from "$lib/stores/viewport";
  import { waypoints, measurements, maslowState } from "$lib/stores/maslow";
  import CalibrationWizard from "$lib/components/CalibrationWizard.svelte";
  import CalibrationView from "$lib/components/CalibrationView.svelte";
  import CalibrationSolver from "$lib/components/CalibrationSolver.svelte";
  import BeltControls from "$lib/components/controls/BeltControls.svelte";

  // The waypoint map + local solver are advanced/desktop tooling; collapse them
  // behind disclosures on a phone where vertical space is scarce.
  const isPhone = $derived($layout === "phone");

  // Only show these modules when they actually have something to display:
  // the map while waypoints exist or calibration is live, the solver once raw
  // measurements have been logged. Matches each component's own hide rule so we
  // don't render an empty titled card.
  const showMap = $derived($waypoints.length > 0 || ($maslowState?.busy ?? false));
  const showSolver = $derived($measurements.length > 0);
</script>

<div class="cal-tab">
  <div class="cols">
    <section class="card">
      <h3>Guided calibration</h3>
      <CalibrationWizard />
    </section>
    <section class="card">
      <h3>Belts</h3>
      <BeltControls />
    </section>
  </div>

  {#if isPhone}
    {#if showMap}
      <details class="card" open>
        <summary>Waypoint map</summary>
        <CalibrationView />
      </details>
    {/if}
    {#if showSolver}
      <details class="card" open>
        <summary>Anchor solver</summary>
        <CalibrationSolver />
      </details>
    {/if}
  {:else}
    {#if showMap}
      <section class="card">
        <h3>Waypoint map</h3>
        <CalibrationView />
      </section>
    {/if}
    {#if showSolver}
      <section class="card">
        <h3>Anchor solver</h3>
        <CalibrationSolver />
      </section>
    {/if}
  {/if}
</div>

<style>
  .cal-tab {
    display: flex;
    flex-direction: column;
    gap: var(--gap-lg);
    padding: var(--gap-lg);
  }
  .cols {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
    gap: var(--gap-lg);
    align-items: start;
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
  details summary {
    cursor: pointer;
    color: var(--text-dim);
    font-size: 0.9em;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    user-select: none;
  }
  details[open] summary {
    margin-bottom: var(--gap);
  }

  @media (max-width: 860px) {
    .cols {
      grid-template-columns: 1fr;
    }
  }
</style>
