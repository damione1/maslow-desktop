import { writable } from "svelte/store";

/** Top-level navigation destinations (single responsive shell). */
export type Tab = "main" | "run" | "calibrate" | "files" | "config";

/** Sub-tabs inside MAIN. */
export type MainSubTab = "jog" | "belts" | "mdi";

const TAB_KEY = "maslow.ui.tab";
const SUBTAB_KEY = "maslow.ui.mainSubTab";
const COLLAPSED_KEY = "maslow.ui.consoleCollapsed";
const HEIGHT_KEY = "maslow.ui.consoleHeight";

export const CONSOLE_MIN_HEIGHT = 96;
export const CONSOLE_DEFAULT_HEIGHT = 220;

const hasStorage = typeof localStorage !== "undefined";

const TABS: Tab[] = ["main", "run", "calibrate", "files", "config"];
const SUBTABS: MainSubTab[] = ["jog", "belts", "mdi"];

function loadTab(): Tab {
  const v = hasStorage ? localStorage.getItem(TAB_KEY) : null;
  return v && (TABS as string[]).includes(v) ? (v as Tab) : "main";
}

function loadSubTab(): MainSubTab {
  const v = hasStorage ? localStorage.getItem(SUBTAB_KEY) : null;
  return v && (SUBTABS as string[]).includes(v) ? (v as MainSubTab) : "jog";
}

function loadCollapsed(): boolean {
  return hasStorage ? localStorage.getItem(COLLAPSED_KEY) === "1" : false;
}

function loadHeight(): number {
  const raw = hasStorage ? localStorage.getItem(HEIGHT_KEY) : null;
  const n = raw ? Number(raw) : NaN;
  return Number.isFinite(n) && n >= CONSOLE_MIN_HEIGHT ? n : CONSOLE_DEFAULT_HEIGHT;
}

/** Active top-level tab. Persisted across reloads. */
export const activeTab = writable<Tab>(loadTab());
/** Active sub-tab within MAIN. Persisted across reloads. */
export const mainSubTab = writable<MainSubTab>(loadSubTab());
/** Whether the bottom console dock is collapsed to a thin bar. */
export const consoleCollapsed = writable<boolean>(loadCollapsed());
/** Expanded console dock height in px (clamped on resize). */
export const consoleHeight = writable<number>(loadHeight());

if (hasStorage) {
  activeTab.subscribe((v) => localStorage.setItem(TAB_KEY, v));
  mainSubTab.subscribe((v) => localStorage.setItem(SUBTAB_KEY, v));
  consoleCollapsed.subscribe((v) => localStorage.setItem(COLLAPSED_KEY, v ? "1" : "0"));
  consoleHeight.subscribe((v) => localStorage.setItem(HEIGHT_KEY, String(Math.round(v))));
}
