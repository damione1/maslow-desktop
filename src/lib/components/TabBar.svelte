<script lang="ts">
  import { activeTab, type Tab } from "$lib/stores/ui";
  import { anchors, maslowState } from "$lib/stores/maslow";

  const tabs: { id: Tab; label: string }[] = [
    { id: "job", label: "Job" },
    { id: "calibrate", label: "Calibrate" },
    { id: "config", label: "Config" },
  ];

  // Speculative discoverability cue: when the machine boots already calibrated
  // and idle in EXTENDEDOUT(4)/RETRACTED(2), a daily-resume action lives under
  // Calibrate. A dot on the tab surfaces it without forcing it as the default.
  const resumeHint = $derived(
    ($anchors?.valid ?? false) &&
      !($maslowState?.busy ?? false) &&
      ($maslowState?.code === 4 || $maslowState?.code === 2),
  );
</script>

<nav class="tabbar">
  {#each tabs as t}
    <button
      class="tab"
      class:active={$activeTab === t.id}
      onclick={() => activeTab.set(t.id)}
    >
      {t.label}
      {#if t.id === "calibrate" && resumeHint}
        <span class="dot" title="Machine calibrated — daily resume available"></span>
      {/if}
    </button>
  {/each}
</nav>

<style>
  .tabbar {
    display: flex;
    gap: 4px;
    padding: 0.5em 0.8em 0;
    background: #161616;
    border-bottom: 1px solid #2a2a2a;
    flex: 0 0 auto;
  }
  .tab {
    position: relative;
    padding: 0.5em 1.1em;
    border: 1px solid transparent;
    border-bottom: none;
    border-radius: 8px 8px 0 0;
    background: transparent;
    color: #aaa;
    cursor: pointer;
    font-size: 0.92em;
    font-weight: 600;
  }
  .tab:hover {
    color: #ddd;
  }
  .tab.active {
    background: #1f1f1f;
    border-color: #333;
    color: #fff;
  }
  .dot {
    position: absolute;
    top: 6px;
    right: 6px;
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: #3ddc84;
  }
</style>
