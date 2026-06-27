<script lang="ts">
  import StatusPanel from "./StatusPanel.svelte";
  import JogControls from "./JogControls.svelte";

  // The mobile/tablet landing: glance-at DRO on top, big touch jog pad below.
  // Manual Control owns Define-home XY/Z + Options, so the primary screen an
  // operator uses standing at the machine is self-contained.
</script>

<div class="control">
  <StatusPanel />
  <JogControls touch />
</div>

<style>
  .control {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
</style>
