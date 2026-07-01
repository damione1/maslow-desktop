<script lang="ts">
  import { loadedJob } from "$lib/stores/job";
  import { activeTab } from "$lib/stores/ui";
  import FileBrowser from "$lib/components/FileBrowser.svelte";

  // FileBrowser already parses the toolpath (loadSdToolpath) before calling back.
  // Record the selection as the loaded job and jump to RUN to start it.
  function onselect(f: { path: string; name: string }) {
    loadedJob.set({ source: "sd", path: f.path, name: f.name });
    activeTab.set("run");
  }
</script>

<div class="files-tab">
  <FileBrowser {onselect} />
</div>

<style>
  .files-tab {
    display: flex;
    flex-direction: column;
    flex: 1;
    padding: var(--gap-lg);
    min-height: 0;
  }
  .files-tab :global(.files) {
    flex: 1;
    min-height: 0;
  }
</style>
