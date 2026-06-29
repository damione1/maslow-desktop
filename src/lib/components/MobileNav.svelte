<script lang="ts">
  import { activeSection, type Section } from "$lib/stores/ui";
  import { anchors, maslowState } from "$lib/stores/maslow";
  import { isResumablePreCut } from "$lib/stores/calState";

  const items: { id: Section; label: string }[] = [
    { id: "control", label: "Control" },
    { id: "job", label: "Job" },
    { id: "calibrate", label: "Setup" },
    { id: "more", label: "More" },
  ];

  // Same daily-resume cue as the desktop tab bar: a dot on Setup when the
  // machine boots calibrated + idle in EXTENDEDOUT(4)/RETRACTED(2).
  const resumeHint = $derived(
    ($anchors?.calibrated ?? false) &&
      !($maslowState?.busy ?? false) &&
      isResumablePreCut($maslowState?.code),
  );
</script>

<nav class="nav">
  {#each items as it}
    <button
      class="item"
      class:active={$activeSection === it.id}
      onclick={() => activeSection.set(it.id)}
    >
      <span class="icon">
        {#if it.id === "control"}
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <path d="M12 2l3 3h-2v5h5V8l3 3-3 3v-2h-5v5h2l-3 3-3-3h2v-5H6v2l-3-3 3-3v2h5V5H9z" />
          </svg>
        {:else if it.id === "job"}
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <path d="M8 5v14l11-7z" />
          </svg>
        {:else if it.id === "calibrate"}
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <path d="M12 8a4 4 0 100 8 4 4 0 000-8zm9 4a7 7 0 00-.13-1.3l2.06-1.6-2-3.46-2.42.98a7 7 0 00-2.26-1.3L15.9 2h-4l-.35 2.62a7 7 0 00-2.26 1.3L6.87 4.94l-2 3.46L6.93 10A7 7 0 006.8 12c0 .44.05.87.13 1.3l-2.06 1.6 2 3.46 2.42-.98c.68.56 1.45 1 2.26 1.3L13.9 22h.2z" opacity="0.9" />
          </svg>
        {:else}
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <path d="M4 10a2 2 0 100 4 2 2 0 000-4zm8 0a2 2 0 100 4 2 2 0 000-4zm8 0a2 2 0 100 4 2 2 0 000-4z" />
          </svg>
        {/if}
        {#if it.id === "calibrate" && resumeHint}
          <span class="dot" title="Calibrated — daily resume available"></span>
        {/if}
      </span>
      <span class="label">{it.label}</span>
    </button>
  {/each}
</nav>

<style>
  .nav {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    background: #141414;
    border-top: 1px solid #2a2a2a;
    /* Sit above the iOS home indicator on notched devices. */
    padding-bottom: env(safe-area-inset-bottom, 0);
  }
  .item {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 3px;
    padding: 0.5em 0;
    min-height: 56px;
    background: transparent;
    border: none;
    color: #888;
    cursor: pointer;
    font: inherit;
  }
  .item.active {
    color: #7fb2ff;
  }
  .icon {
    position: relative;
    display: inline-flex;
  }
  .icon svg {
    width: 24px;
    height: 24px;
    fill: currentColor;
  }
  .label {
    font-size: 0.72em;
    font-weight: 600;
  }
  .dot {
    position: absolute;
    top: -2px;
    right: -4px;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: #3ddc84;
  }
</style>
