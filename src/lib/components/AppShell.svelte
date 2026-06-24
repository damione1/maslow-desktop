<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    topbar,
    workspace,
    rail,
    dock,
  }: {
    topbar: Snippet;
    workspace: Snippet;
    rail: Snippet;
    dock: Snippet;
  } = $props();
</script>

<div class="shell">
  <header class="zone topbar">{@render topbar()}</header>
  <main class="zone workspace">{@render workspace()}</main>
  <aside class="zone rail">{@render rail()}</aside>
  <footer class="zone dock">{@render dock()}</footer>
</div>

<style>
  .shell {
    display: grid;
    height: 100vh;
    grid-template-columns: 1fr 340px;
    grid-template-rows: auto 1fr auto;
    grid-template-areas:
      "topbar topbar"
      "workspace rail"
      "dock dock";
  }
  .zone {
    min-width: 0;
    min-height: 0;
  }
  .topbar {
    grid-area: topbar;
  }
  .workspace {
    grid-area: workspace;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .rail {
    grid-area: rail;
    /* Scroll vertically only if the content (DRO + jog + Hold/Resume/Reset)
       truly overflows — e.g. when the console dock is dragged tall. No phantom
       horizontal bar, and the realtime buttons are never clipped. */
    overflow-y: auto;
    overflow-x: hidden;
    border-left: 1px solid #2a2a2a;
    background: #141414;
  }
  .dock {
    grid-area: dock;
  }

  /* Small screens / short laptops: single column. The rail drops between the
     workspace and the console so DRO + jog stay visible without scrolling the
     shell itself. */
  @media (max-width: 820px) {
    /* Stacked single column: let the document scroll as ONE level instead of
       squeezing the workspace to 0 and double-scrolling. The shell grows past
       the viewport and the zones keep their natural height. */
    .shell {
      height: auto;
      min-height: 100vh;
      grid-template-columns: 1fr;
      grid-template-rows: auto auto auto auto;
      grid-template-areas:
        "topbar"
        "workspace"
        "rail"
        "dock";
    }
    .workspace {
      overflow: visible;
    }
    .rail {
      overflow: visible;
      border-left: none;
      border-top: 1px solid #2a2a2a;
    }
  }
</style>
