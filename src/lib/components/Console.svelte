<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import {
    consoleLines,
    clearConsole,
    pushConsoleLine,
    wsState,
  } from "$lib/stores/machine";
  import { jobProgress } from "$lib/stores/job";

  let viewport: HTMLDivElement | undefined = $state();
  let autoscroll = $state(true);

  // Free-text command line: send arbitrary `$`/G-code for diagnostics ($#, $G,
  // $$, per-belt commands, …). Blocked during a job (a stray line would corrupt
  // the streamer's char-counting — the backend rejects it too).
  let cmd = $state("");
  let history = $state<string[]>([]);
  let histIndex = $state(-1); // -1 = editing a fresh line

  const connected = $derived($wsState === "connected");
  const jobActive = $derived(
    $jobProgress !== null &&
      $jobProgress.state !== "done" &&
      $jobProgress.state !== "error",
  );
  const canSend = $derived(connected && !jobActive);

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

  function send() {
    const line = cmd.trim();
    if (!line || !canSend) return;
    pushConsoleLine(`› ${line}`); // local echo so the user sees what was sent
    invoke("send_line", { line });
    history = [line, ...history.filter((h) => h !== line)].slice(0, 50);
    histIndex = -1;
    cmd = "";
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      send();
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      if (histIndex + 1 < history.length) {
        histIndex += 1;
        cmd = history[histIndex];
      }
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      if (histIndex > 0) {
        histIndex -= 1;
        cmd = history[histIndex];
      } else {
        histIndex = -1;
        cmd = "";
      }
    }
  }
</script>

<section class="console">
  <header>
    <span>Console</span>
    <button onclick={clearConsole}>Clear</button>
  </header>
  <div class="lines" bind:this={viewport} onscroll={onScroll}>
    {#each $consoleLines as line}
      <div class="line" class:sent={line.startsWith("› ")} class:msg={line.startsWith("[MSG")} class:err={line.startsWith("error") || line.startsWith("ALARM") || line.startsWith("[ws error]")}>
        {line}
      </div>
    {/each}
  </div>
  <div class="cmdline">
    <span class="prompt">›</span>
    <input
      type="text"
      bind:value={cmd}
      onkeydown={onKey}
      disabled={!canSend}
      placeholder={!connected
        ? "connect to send commands"
        : jobActive
          ? "stop the job to send commands"
          : "type a command (e.g. $G, $#, $$) — Enter to send, ↑ history"}
      spellcheck="false"
      autocapitalize="off"
      autocomplete="off"
    />
    <button onclick={send} disabled={!canSend || cmd.trim() === ""}>Send</button>
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
  .line.sent {
    color: #9be29b;
    opacity: 0.9;
  }
  .cmdline {
    display: flex;
    align-items: center;
    gap: 0.5em;
    padding: 0.45em 0.7em;
    border-top: 1px solid #2a2a2a;
    flex: 0 0 auto;
  }
  .cmdline .prompt {
    color: #9be29b;
    font-family: "SF Mono", Menlo, Consolas, monospace;
    opacity: 0.7;
  }
  .cmdline input {
    flex: 1;
    min-width: 0;
    background: #161616;
    border: 1px solid #333;
    border-radius: 6px;
    color: #eee;
    padding: 0.35em 0.6em;
    font-family: "SF Mono", Menlo, Consolas, monospace;
    font-size: 0.8em;
  }
  .cmdline input:disabled {
    opacity: 0.5;
  }
  .cmdline button {
    font-size: 0.8em;
    padding: 0.3em 0.8em;
    border-radius: 6px;
    border: 1px solid #396cd8;
    background: #396cd8;
    color: #fff;
    cursor: pointer;
  }
  .cmdline button:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
</style>
