/**
 * Geometry: how wide a region actually renders. Content declares an appetite,
 * the slot clamps it, and the breakpoint overrides both. That order is the
 * rule — content never wins against the available space.
 */

import type { Registry, ShellState, Slot } from './types.ts';

export type Breakpoint = 'sm' | 'md' | 'lg' | 'xl';

/*
  Widths in CSS px at the shell's UI scale (root font-size 20px, i.e. 125% of
  the design's 16px base — DESIGN.md §12.2). Text and spacing in the shell are
  rem, so they scale with that one number; these model widths carry the same
  scale explicitly.
*/
export const RAIL_WIDTH = 90;
export const RAIL_COMPACT = 60;
/** The rail at full width — same navigation, labels instead of icons. */
export const NAV_WIDTH = 310;
/** The gap between the rail and the panel beside it. */
export const RAIL_GAP = 9;
/** The centre never shrinks below this; side panes give way instead. */
export const CENTRE_MIN = 700;

export function breakpointFor(width: number): Breakpoint {
  if (width >= 1600) return 'xl';
  if (width >= 1280) return 'lg';
  if (width >= 960) return 'md';
  return 'sm';
}

/**
 * A compact rail drops the label and leans on the tooltip, so it is only
 * honest where a pointer exists. On touch, a narrow viewport gets the drawer
 * instead — bigger targets and real labels, not smaller ones.
 */
export function railWidthFor(bp: Breakpoint, coarsePointer: boolean): number {
  return bp === 'md' && !coarsePointer ? RAIL_COMPACT : RAIL_WIDTH;
}

export function clamp(value: number, lo: number, hi: number): number {
  return Math.min(hi, Math.max(lo, value));
}

/** The width appetite of whatever is currently in a pane. */
export function contentBounds(registry: Registry, ids: readonly string[]): { min: number; max: number } {
  if (!ids.length) return { min: 0, max: 0 };
  const specs = ids.map(id => registry[id]).filter(Boolean);
  if (!specs.length) return { min: 0, max: 0 };
  return {
    min: Math.max(...specs.map(s => s.min)),
    max: Math.min(...specs.map(s => s.max)),
  };
}

export function sideWidth(
  registry: Registry,
  state: ShellState,
  slot: 'left' | 'right',
  bp: Breakpoint,
  railW: number,
): number {
  if (bp === 'sm') return 0;
  const pane = state[slot];
  if (pane.mode === 'hidden') return 0;

  /* Navigation width and panel width are independent. The left region shows
     navigation at one of two widths, and a panel beside it when one is open;
     neither hides the other. */
  const nav = pane.mode === 'nav' ? NAV_WIDTH + RAIL_GAP
    : pane.mode === 'rail' ? railW + RAIL_GAP
    : 0;
  if (!pane.panels.length) return nav;
  const { min, max } = contentBounds(registry, pane.panels);
  return nav + clamp(pane.size, min, max);
}

export function bottomHeight(state: ShellState): number {
  return state.bottom.panels.length ? state.bottom.size : 0;
}

/**
 * Side panes may take any width the user drags to, as long as the centre keeps
 * CENTRE_MIN. One rule that scales by itself: generous on an ultrawide,
 * protective at 1280, with no per-breakpoint maximums to maintain.
 */
export function fitToViewport(viewport: number, left: number, right: number): { left: number; right: number } {
  const over = left + right + CENTRE_MIN - viewport;
  if (over <= 0) return { left, right };
  const total = left + right;
  if (total <= 0) return { left, right };
  return {
    left: Math.max(0, Math.round(left - over * (left / total))),
    right: Math.max(0, Math.round(right - over * (right / total))),
  };
}
