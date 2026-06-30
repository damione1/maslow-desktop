<script lang="ts">
  import { machineStatus, stateClass } from "$lib/stores/machine";

  let { compact = false }: { compact?: boolean } = $props();

  const s = $derived($machineStatus);
  const label = $derived(
    s ? `${s.state}${s.substate !== null ? `:${s.substate}` : ""}` : "Waiting for status…",
  );
  const cls = $derived(s ? stateClass(s.state) : "other");
</script>

<div class="state-bar {cls}" class:compact>
  {label}
</div>

<style>
  /* A status banner, not a touch target: keep it slim regardless of --tap. */
  .state-bar {
    display: flex;
    align-items: center;
    justify-content: center;
    flex: 0 0 auto;
    width: 100%;
    min-height: 38px;
    padding: 0.3em 1em;
    border-radius: var(--radius);
    font-weight: 700;
    font-size: 1.05em;
    letter-spacing: 0.02em;
    color: #fff;
    background: var(--state-other);
  }
  .state-bar.compact {
    min-height: 30px;
    font-size: 0.9em;
  }
  .state-bar.idle {
    background: var(--state-idle);
  }
  .state-bar.run {
    background: var(--state-run);
  }
  .state-bar.hold {
    background: var(--state-hold);
  }
  .state-bar.alarm {
    background: var(--state-alarm);
  }
  .state-bar.other {
    background: var(--state-other);
  }
</style>
