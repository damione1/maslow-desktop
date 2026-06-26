import { writable } from "svelte/store";

export type Tab = "job" | "calibrate" | "config";

/** Mobile/tablet primary nav destinations. Control (manual jog) is the landing
 * section; secondary config + connection live under More. */
export type Section = "control" | "job" | "calibrate" | "more";

const TAB_KEY = "maslow.ui.tab";
const SECTION_KEY = "maslow.ui.section";
const COLLAPSED_KEY = "maslow.ui.consoleCollapsed";
const HEIGHT_KEY = "maslow.ui.consoleHeight";

export const CONSOLE_MIN_HEIGHT = 96;
export const CONSOLE_DEFAULT_HEIGHT = 220;

const hasStorage = typeof localStorage !== "undefined";

function loadTab(): Tab {
  const v = hasStorage ? localStorage.getItem(TAB_KEY) : null;
  return v === "calibrate" || v === "config" ? v : "job";
}

function loadSection(): Section {
  const v = hasStorage ? localStorage.getItem(SECTION_KEY) : null;
  return v === "job" || v === "calibrate" || v === "more" ? v : "control";
}

function loadCollapsed(): boolean {
  return hasStorage ? localStorage.getItem(COLLAPSED_KEY) === "1" : false;
}

function loadHeight(): number {
  const raw = hasStorage ? localStorage.getItem(HEIGHT_KEY) : null;
  const n = raw ? Number(raw) : NaN;
  return Number.isFinite(n) && n >= CONSOLE_MIN_HEIGHT ? n : CONSOLE_DEFAULT_HEIGHT;
}

/** Active workspace tab (desktop). Persisted across reloads. */
export const activeTab = writable<Tab>(loadTab());
/** Active mobile/tablet section. Defaults to Control (manual jog landing). */
export const activeSection = writable<Section>(loadSection());
/** Whether the bottom console dock is collapsed to a thin bar. */
export const consoleCollapsed = writable<boolean>(loadCollapsed());
/** Expanded console dock height in px (clamped on resize). */
export const consoleHeight = writable<number>(loadHeight());

if (hasStorage) {
  activeTab.subscribe((v) => localStorage.setItem(TAB_KEY, v));
  activeSection.subscribe((v) => localStorage.setItem(SECTION_KEY, v));
  consoleCollapsed.subscribe((v) => localStorage.setItem(COLLAPSED_KEY, v ? "1" : "0"));
  consoleHeight.subscribe((v) => localStorage.setItem(HEIGHT_KEY, String(Math.round(v))));
}
