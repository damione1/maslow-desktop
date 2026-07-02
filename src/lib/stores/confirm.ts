import { writable } from "svelte/store";

export interface ConfirmRequest {
  message: string;
  title: string;
  confirmLabel: string;
  cancelLabel: string;
  danger: boolean;
  resolve: (result: boolean) => void;
}

// Single-slot: this app only ever has one user-driven confirmable action in
// flight at a time, so a queue would be unused complexity.
export const confirmRequest = writable<ConfirmRequest | null>(null);

export function confirmDialog(
  message: string,
  opts?: {
    title?: string;
    confirmLabel?: string;
    cancelLabel?: string;
    danger?: boolean;
  },
): Promise<boolean> {
  return new Promise((resolve) => {
    confirmRequest.set({
      message,
      title: opts?.title ?? "Confirm",
      confirmLabel: opts?.confirmLabel ?? "Confirm",
      cancelLabel: opts?.cancelLabel ?? "Cancel",
      danger: opts?.danger ?? false,
      resolve,
    });
  });
}

export function resolveConfirm(result: boolean) {
  confirmRequest.update((req) => {
    req?.resolve(result);
    return null;
  });
}
