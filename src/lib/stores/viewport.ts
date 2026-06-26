import { readable, derived } from "svelte/store";

export type Layout = "phone" | "tablet" | "desktop";

// Breakpoints chosen where the layout actually breaks, not per-device:
// phone keeps a single column + bottom tabs, tablet gets the mobile shell with
// roomier 2-column sections, desktop (≥1024) keeps the original 3-zone shell.
const PHONE_MAX = 599;
const TABLET_MAX = 1023;

function compute(w: number): Layout {
  if (w <= PHONE_MAX) return "phone";
  if (w <= TABLET_MAX) return "tablet";
  return "desktop";
}

/** Current layout class, recomputed on resize. SSR-safe (defaults to desktop). */
export const layout = readable<Layout>(
  typeof window !== "undefined" ? compute(window.innerWidth) : "desktop",
  (set) => {
    if (typeof window === "undefined") return;
    const onResize = () => set(compute(window.innerWidth));
    onResize();
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  },
);

/** True on phone + tablet — drives the touch-optimized mobile shell. */
export const isMobile = derived(layout, ($l) => $l !== "desktop");
