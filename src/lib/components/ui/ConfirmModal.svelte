<script lang="ts">
  import { confirmRequest, resolveConfirm } from "$lib/stores/confirm";
  import Modal from "$lib/components/ui/Modal.svelte";
  import Button from "$lib/components/ui/Button.svelte";

  const req = $derived($confirmRequest);
</script>

{#if req}
  <Modal title={req.title} onclose={() => resolveConfirm(false)}>
    <p class="message">{req.message}</p>
    <div class="actions">
      <Button variant="ghost" onclick={() => resolveConfirm(false)}>{req.cancelLabel}</Button>
      <Button variant={req.danger ? "danger" : "action"} onclick={() => resolveConfirm(true)}>
        {req.confirmLabel}
      </Button>
    </div>
  </Modal>
{/if}

<style>
  .message {
    margin: 0 0 1em;
    white-space: pre-wrap;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 0.6em;
  }
</style>
