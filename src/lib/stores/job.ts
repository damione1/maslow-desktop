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

export interface ToolpathSegment {
  x0: number;
  y0: number;
  x1: number;
  y1: number;
  rapid: boolean;
  line: number;
}

export interface Toolpath {
  segments: ToolpathSegment[];
  min_x: number;
  min_y: number;
  max_x: number;
  max_y: number;
  has_bounds: boolean;
}

/** Parsed 2D toolpath of the currently selected local file, or null. */
export const toolpath = writable<Toolpath | null>(null);
/** Path the current toolpath was parsed from, to match against a running job. */
export const toolpathPath = writable<string | null>(null);

/** Parse a local G-code file into a toolpath for preview + trace boundary. */
export async function loadToolpath(path: string): Promise<void> {
  try {
    toolpath.set(await invoke<Toolpath>("load_toolpath", { path }));
    toolpathPath.set(path);
  } catch {
    toolpath.set(null);
    toolpathPath.set(null);
  }
}

/** Download an SD-card file and parse its toolpath for preview — without
 * running it. Lets the operator check where a job sits on the board first. */
export async function loadSdToolpath(host: string, path: string): Promise<void> {
  try {
    toolpath.set(await invoke<Toolpath>("sd_toolpath", { host, path }));
    toolpathPath.set(`SD:${path}`);
  } catch {
    toolpath.set(null);
    toolpathPath.set(null);
  }
}

/** Drop the previewed toolpath (used when the operator unloads the job). */
export function clearToolpath(): void {
  toolpath.set(null);
  toolpathPath.set(null);
}

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
