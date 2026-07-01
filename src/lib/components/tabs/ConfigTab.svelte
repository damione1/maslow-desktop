<script lang="ts">
  import MaslowConfig from "$lib/components/MaslowConfig.svelte";
  import FluidNCConfig from "$lib/components/FluidNCConfig.svelte";
</script>

<div class="config-tab">
  <section class="card">
    <h3>Maslow settings</h3>
    <MaslowConfig />
  </section>
  <section class="card">
    <h3>FluidNC config</h3>
    <FluidNCConfig />
  </section>
</div>

<style>
  .config-tab {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
    gap: var(--gap-lg);
    padding: var(--gap-lg);
    align-items: start;
  }
  .card {
    background: var(--surface);
    border: 1px solid var(--border-2);
    border-radius: var(--radius-lg);
    padding: var(--gap-lg);
    min-width: 0;
  }
  h3 {
    margin: 0 0 var(--gap) 0;
    font-size: 0.9em;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-dim);
  }
  @media (max-width: 980px) {
    .config-tab {
      grid-template-columns: 1fr;
    }
  }
</style>
