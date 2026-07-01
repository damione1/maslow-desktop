<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    topbar,
    content,
    statusbar,
  }: {
    topbar: Snippet;
    content: Snippet;
    statusbar: Snippet;
  } = $props();
</script>

<div class="frame">
  <div class="zone top">{@render topbar()}</div>
  <div class="zone content">{@render content()}</div>
  <div class="zone bottom">{@render statusbar()}</div>
</div>

<style>
  .frame {
    display: flex;
    flex-direction: column;
    height: 100dvh;
    width: 100%;
    overflow: hidden;
    background: var(--bg);
    color: var(--text);
  }
  .zone.top {
    flex: 0 0 auto;
    padding-top: env(safe-area-inset-top, 0);
  }
  .zone.content {
    flex: 1 1 auto;
    min-height: 0;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }
  .zone.bottom {
    flex: 0 0 auto;
    padding-bottom: env(safe-area-inset-bottom, 0);
  }
</style>
