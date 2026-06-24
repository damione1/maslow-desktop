<script lang="ts">
  let { children } = $props();
</script>

{@render children()}

<style>
  /* App-wide themed scrollbars matching the dark palette (#0e0e0e..#222,
     borders #2a2a2a/#333). Applies to the console, the rail, tab scroll
     areas, and any other overflow:auto region so nothing falls back to the
     default OS scrollbars. */

  /* Firefox */
  :global(*) {
    scrollbar-width: thin;
    scrollbar-color: #3a3a3a transparent;
  }

  /* WebKit / Chromium (Tauri uses the system WebView) */
  :global(::-webkit-scrollbar) {
    width: 10px;
    height: 10px;
  }
  :global(::-webkit-scrollbar-track) {
    background: transparent;
  }
  :global(::-webkit-scrollbar-thumb) {
    background: #3a3a3a;
    border-radius: 8px;
    /* Inset the thumb so it reads as a thin pill, not a full-width bar. */
    border: 2px solid transparent;
    background-clip: padding-box;
  }
  :global(::-webkit-scrollbar-thumb:hover) {
    background: #555;
    background-clip: padding-box;
  }
  :global(::-webkit-scrollbar-corner) {
    background: transparent;
  }
</style>
