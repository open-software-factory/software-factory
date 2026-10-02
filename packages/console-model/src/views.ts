/**
 * Per-view layout (DESIGN.md §5.2.3): each view remembers *which* side and
 * bottom panels it was last seen with. Navigating to a view brings them back;
 * navigating away stores them. A view never visited keeps whatever is open,
 * so the first visit is not a jarring reset.
 *
 * Widths are global. A resize says how much room the operator wants on this
 * screen, not something about the view; a seam that jumped on every switch
 * would read as the layout shoving itself. Navigation width (rail or wide)
 * is global for the same reason. What differs between views is presence.
 *
 * Only *navigation* switches memory — open/activate/close in the centre.
 * Moving a surface between slots or restoring a whole layout (focus mode)
 * changes the layout on purpose and must not be second-guessed by a recall.
 */

import { reduce } from './layout.ts';
import { placementOf } from './placement.ts';
import type { Action, PaneState, Registry, ShellState, SurfaceId } from './types.ts';

export type SidePanes = Pick<ShellState, 'left' | 'right' | 'bottom'>;
export type ViewMemory = Readonly<Record<SurfaceId, SidePanes>>;

/** A pane the operator can place a surface into on purpose. */
export type PlacedSlot = 'left' | 'right' | 'bottom';
/** Where the operator moved a surface. Wins over per-view memory until closed or moved again. */
export type Placement = Readonly<Record<SurfaceId, PlacedSlot>>;

export interface Workspace {
  layout: ShellState;
  views: ViewMemory;
  placed: Placement;
}

export const sidesOf = (s: ShellState): SidePanes => ({ left: s.left, right: s.right, bottom: s.bottom });

const NAVIGATION = new Set<Action['type']>(['open', 'activate', 'closeSurface', 'closePane', 'toggleSurface']);

/** A remembered pane may name a surface that has since moved into the centre; it cannot be in both. */
function without(pane: PaneState, taken: readonly SurfaceId[]): PaneState {
  const panels = pane.panels.filter(id => !taken.includes(id));
  if (panels.length === pane.panels.length) return pane;
  return { ...pane, panels, active: pane.active && panels.includes(pane.active) ? pane.active : (panels[0] ?? null) };
}

/**
 * Presence comes from memory; size and navigation width stay as they are.
 * A region's hidden/shown mode follows presence: a remembered panel reveals
 * its region, an empty memory hides it (the left keeps its rail).
 */
function place(slot: 'left' | 'right' | 'bottom', current: PaneState, remembered: PaneState, taken: readonly SurfaceId[]): PaneState {
  const wanted = without(remembered, taken);
  const mode = slot === 'left'
    ? current.mode
    : wanted.panels.length ? 'expanded' : 'hidden';
  return { ...current, panels: wanted.panels, active: wanted.active, mode };
}

export function recall(layout: ShellState, remembered: SidePanes | undefined): ShellState {
  if (!remembered) return layout;
  const taken = layout.centre.panels;
  return {
    ...layout,
    left: place('left', layout.left, remembered.left, taken),
    right: place('right', layout.right, remembered.right, taken),
    bottom: place('bottom', layout.bottom, remembered.bottom, taken),
  };
}

/** A 'move' to a side records or clears an entry; any other action drops one whose surface left its slot. */
function updatePlaced(placed: Placement, layout: ShellState, action: Action): Placement {
  if (action.type === 'move') {
    if (action.to === 'left' || action.to === 'right' || action.to === 'bottom') {
      return { ...placed, [action.id]: action.to };
    }
    if (!(action.id in placed)) return placed;
    const next = { ...placed };
    delete next[action.id];
    return next;
  }
  let changed = false;
  const next: Record<SurfaceId, PlacedSlot> = {};
  for (const [id, slot] of Object.entries(placed)) {
    if (placementOf(layout, id) === slot) next[id] = slot;
    else changed = true;
  }
  return changed ? next : placed;
}

/** Drop `id` from `pane`; the right region collapses when that empties it, matching layout.ts's own rule. */
function vacate(slot: PlacedSlot, pane: PaneState, id: SurfaceId): PaneState {
  const removed = without(pane, [id]);
  if (removed === pane) return pane;
  if (slot === 'right' && removed.panels.length === 0) return { ...removed, mode: 'hidden' };
  return removed;
}

/** Put `id` in `pane`, revealing the region the same way layout.ts's addTo does for a fresh open. */
function placeId(slot: PlacedSlot, pane: PaneState, id: SurfaceId): PaneState {
  const panels = pane.panels.includes(id) ? pane.panels : [...pane.panels, id];
  const active = pane.active ?? id;
  const mode = slot === 'left' ? (pane.mode === 'hidden' ? 'rail' : pane.mode) : 'expanded';
  return { ...pane, panels, active, mode };
}

/** Placement wins over whatever recall just restored: put every placed surface back where the operator left it. */
function applyPlacements(layout: ShellState, placed: Placement): ShellState {
  let out = layout;
  for (const [id, slot] of Object.entries(placed)) {
    const at = placementOf(out, id);
    if (at === slot) continue;
    if (at === 'left' || at === 'right' || at === 'bottom') out = { ...out, [at]: vacate(at, out[at], id) };
    out = { ...out, [slot]: placeId(slot, out[slot], id) };
  }
  return out;
}

export function reduceWorkspace(registry: Registry, ws: Workspace, action: Action): Workspace {
  const layout = reduce(registry, ws.layout, action);
  if (layout === ws.layout) return ws;
  const placed = updatePlaced(ws.placed, layout, action);
  const from = ws.layout.centre.active;
  const to = layout.centre.active;
  if (from === to || !NAVIGATION.has(action.type)) return { layout, views: ws.views, placed };
  const views: ViewMemory = from ? { ...ws.views, [from]: sidesOf(ws.layout) } : ws.views;
  const recalled = recall(layout, to ? views[to] : undefined);
  return { layout: applyPlacements(recalled, placed), views, placed };
}
