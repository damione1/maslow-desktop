<script lang="ts">
  import { consoleLines, clearConsole } from "$lib/stores/machine";

  let viewport: HTMLDivElement | undefined = $state();
  let autoscroll = $state(true);

  // Auto-scroll to bottom when new lines arrive, unless the user scrolled up.
  $effect(() => {
    void $consoleLines.length;
    if (autoscroll && viewport) {
      viewport.scrollTop = viewport.scrollHeight;
    }
  });

  function onScroll() {
    if (!viewport) return;
    const atBottom =
      viewport.scrollHeight - viewport.scrollTop - viewport.clientHeight < 20;
    autoscroll = atBottom;
  }
</script>

<section class="console">
  <header>
    <span>Console</span>
    <button onclick={clearConsole}>Clear</button>
  </header>
  <div class="lines" bind:this={viewport} onscroll={onScroll}>
    {#each $consoleLines as line}
      <div class="line" class:msg={line.startsWith("[MSG")} class:err={line.startsWith("error") || line.startsWith("ALARM") || line.startsWith("[ws error]")}>
        {line}
      </div>
    {/each}
  </div>
</section>

<style>
  .console {
    background: #0e0e0e;
    border: 1px solid #333;
    border-radius: 10px;
    display: flex;
    flex-direction: column;
    height: 100%;
    width: 100%;
    min-height: 0;
    /* Clip children to the rounded border so nothing pokes past the corners. */
    overflow: hidden;
  }
  header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 0.8em;
    padding: 0.55em 0.9em;
    border-bottom: 1px solid #2a2a2a;
    font-size: 0.85em;
    opacity: 0.8;
    flex: 0 0 auto;
  }
  header button {
    font-size: 0.8em;
    padding: 0.2em 0.7em;
    border-radius: 6px;
    border: 1px solid #444;
    background: #222;
    color: #ddd;
    cursor: pointer;
  }
  .lines {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    overflow-x: hidden;
    padding: 0.6em 0.9em 0.7em;
    font-family: "SF Mono", Menlo, Consolas, monospace;
    font-size: 0.78em;
    line-height: 1.45;
  }
  .line {
    white-space: pre-wrap;
    word-break: break-word;
  }
  .line.msg {
    color: #7fb2ff;
  }
  .line.err {
    color: #ff6b6b;
  }
</style>
