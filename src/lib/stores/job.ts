import { writable } from "svelte/store";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";

export interface JobProgress {
  state: "running" | "paused" | "interrupted" | "done" | "error" | "idle";
  path: string;
  name: string;
  sent: number;
  acked: number;
  total: number;
  errors: number;
}

export interface SavedJob {
  path: string;
  name: string;
  total: number;
  acked: number;
  state: string;
  updated_at: number;
}

/** Live streaming progress, or null when no job is loaded. */
export const jobProgress = writable<JobProgress | null>(null);

let started = false;

export async function initJobListeners(): Promise<void> {
  if (started) return;
  started = true;
  await listen<JobProgress>("stream-progress", (e) => {
    jobProgress.set(e.payload.state === "idle" ? null : e.payload);
  });
}

/** A resumable job persisted on disk from a previous (interrupted) session. */
export async function loadSavedJob(): Promise<SavedJob | null> {
  return await invoke<SavedJob | null>("stream_saved");
}
