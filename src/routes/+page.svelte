<script lang="ts">
  import { onMount } from "svelte";
  import { initMachineListeners } from "$lib/stores/machine";
  import { initJobListeners } from "$lib/stores/job";
  import { initMaslowListeners } from "$lib/stores/maslow";
  import { activeTab } from "$lib/stores/ui";
  import { layout } from "$lib/stores/viewport";
  import AppFrame from "$lib/components/shell/AppFrame.svelte";
  import TopBar from "$lib/components/shell/TopBar.svelte";
  import StatusStrip from "$lib/components/shell/StatusStrip.svelte";
  import MainTab from "$lib/components/tabs/MainTab.svelte";
  import RunTab from "$lib/components/tabs/RunTab.svelte";
  import CalibrateTab from "$lib/components/tabs/CalibrateTab.svelte";
  import FilesTab from "$lib/components/tabs/FilesTab.svelte";
  import ConfigTab from "$lib/components/tabs/ConfigTab.svelte";
  import SettingsTab from "$lib/components/tabs/SettingsTab.svelte";
  import ConfirmModal from "$lib/components/ui/ConfirmModal.svelte";

  onMount(() => {
    initMachineListeners();
    initJobListeners();
    initMaslowListeners();
  });

  // Mark the document as touch so tokens widen hit targets on phone/tablet.
  $effect(() => {
    if (typeof document === "undefined") return;
    document.documentElement.dataset.touch = $layout === "desktop" ? "0" : "1";
  });
</script>

<AppFrame>
  {#snippet topbar()}
    <TopBar />
  {/snippet}

  {#snippet content()}
    <!-- Tabs stay mounted and hidden with display:none (not {#if}) so canvases
         keep their drawing and config/file panels don't refetch on tab switch. -->
    <div class="panel" class:active={$activeTab === "main"}><MainTab /></div>
    <div class="panel" class:active={$activeTab === "run"}><RunTab /></div>
    <div class="panel" class:active={$activeTab === "calibrate"}><CalibrateTab /></div>
    <div class="panel" class:active={$activeTab === "files"}><FilesTab /></div>
    <div class="panel" class:active={$activeTab === "config"}><ConfigTab /></div>
    <div class="panel" class:active={$activeTab === "settings"}><SettingsTab /></div>
  {/snippet}

  {#snippet statusbar()}
    <StatusStrip />
  {/snippet}
</AppFrame>

<ConfirmModal />

<style>
  :global(body) {
    margin: 0;
    font-family: var(--font);
    color: var(--text);
    background: var(--bg);
  }

  .panel {
    display: none;
    flex: 1;
    min-height: 0;
    overflow: auto;
    scrollbar-gutter: stable;
  }
  .panel.active {
    display: flex;
    flex-direction: column;
  }
</style>
