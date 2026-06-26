<script lang="ts">
  import StatusPanel from "./StatusPanel.svelte";
  import JogControls from "./JogControls.svelte";
  import ControlOptions from "./ControlOptions.svelte";

  // The mobile/tablet landing: glance-at DRO on top, big touch jog pad below,
  // with Set XY/Z Home + Options reachable from the same screen. This is the
  // primary screen an operator uses standing at the machine.
</script>

<div class="control">
  <StatusPanel />
  <JogControls touch />
  <ControlOptions />
</div>

<style>
  .control {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
</style>
