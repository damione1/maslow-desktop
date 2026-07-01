<script lang="ts">
  type Item = { id: string; label: string };

  let {
    items,
    active = $bindable(),
    onchange = undefined,
  }: {
    items: Item[];
    active: string;
    onchange?: (id: string) => void;
  } = $props();

  function select(id: string) {
    active = id;
    onchange?.(id);
  }
</script>

<div class="subtabs" role="tablist">
  {#each items as item}
    <button
      role="tab"
      aria-selected={active === item.id}
      class="subtab"
      class:active={active === item.id}
      onclick={() => select(item.id)}
    >
      {item.label}
    </button>
  {/each}
</div>

<style>
  .subtabs {
    display: flex;
    gap: 2px;
    background: var(--border);
    border: 1px solid var(--border-2);
    border-radius: var(--radius);
    overflow: hidden;
  }
  .subtab {
    flex: 1;
    min-height: var(--tap);
    border: none;
    background: var(--surface-2);
    color: var(--text-dim);
    font-family: var(--font);
    font-weight: 600;
    font-size: 0.95em;
    letter-spacing: 0.03em;
    cursor: pointer;
    transition:
      background 0.12s ease,
      color 0.12s ease;
  }
  .subtab:hover {
    background: var(--surface-3);
    color: var(--text);
  }
  .subtab.active {
    background: var(--action);
    color: var(--action-text);
  }
</style>
