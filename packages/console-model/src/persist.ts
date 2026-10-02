/**
 * Turning layout into bytes and back. The *where* is the storage port's
 * problem (storage.ts); this file only decides what a valid layout is.
 *
 * Restoring is a merge, not a trust exercise: a stored layout may name a
 * surface that no longer exists, or place one somewhere it no longer accepts.
 * Both must degrade to a working shell rather than throw.
 */

import { SLOTS } from './types.ts';
import type { PaneState, Registry, ShellState, Slot } from './types.ts';
import { emptyPane, emptyState } from './layout.ts';
import { canAccept } from './placement.ts';
import type { StoragePort } from './storage.ts';
import type { PlacedSlot, Placement, SidePanes, ViewMemory, Workspace } from './views.ts';

const PLACED_SLOTS: readonly PlacedSlot[] = ['left', 'right', 'bottom'];

export const STORAGE_KEY = 'od-factory-shell';

export interface Persisted {
  version: 1;
  layout: ShellState;
  /** Per-view side panes (§5.2.3). Optional: older payloads have none. */
  views?: ViewMemory;
  /** Operator placements (§5.2.3). Optional: older payloads have none. */
  placed?: Placement;
}

export function serialise(state: ShellState): string {
  return JSON.stringify({ version: 1, layout: state } satisfies Persisted);
}

export function serialiseWorkspace(ws: Workspace): string {
  return JSON.stringify({ version: 1, layout: ws.layout, views: ws.views, placed: ws.placed } satisfies Persisted);
}

/** One pane, merged against what exists now. Unknown or unplaceable surfaces are dropped. */
function mergePane(registry: Registry, slot: Slot, stored: unknown): PaneState | null {
  const s = stored as Partial<PaneState> | undefined;
  if (!s || !Array.isArray(s.panels)) return null;
  const panels = s.panels.filter(
    (id): id is string => typeof id === 'string' && !!registry[id] && canAccept(registry[id], slot),
  );
  const fallback = emptyPane(slot);
  return {
    panels,
    active: panels.includes(s.active as string) ? (s.active as string) : (panels[0] ?? null),
    mode: s.mode ?? fallback.mode,
    size: typeof s.size === 'number' && s.size >= 0 ? s.size : fallback.size,
  };
}

function parse(raw: string | null): Partial<Persisted> | null {
  if (!raw) return null;
  try {
    return JSON.parse(raw) as Partial<Persisted>;
  } catch {
    return null;
  }
}

function mergeLayout(registry: Registry, layout: unknown): ShellState | null {
  if (!layout || typeof layout !== 'object') return null;
  const out: Record<string, unknown> = { ...emptyState() };
  for (const slot of SLOTS) {
    const pane = mergePane(registry, slot, (layout as Record<string, unknown>)[slot]);
    if (pane) out[slot] = pane;
  }
  return out as ShellState;
}

export function deserialise(registry: Registry, raw: string | null): ShellState | null {
  return mergeLayout(registry, parse(raw)?.layout);
}

/** A remembered view must name a surface that exists; each of its panes merges like the layout does. */
export function deserialiseWorkspace(registry: Registry, raw: string | null): Workspace | null {
  const parsed = parse(raw);
  const layout = mergeLayout(registry, parsed?.layout);
  if (!layout) return null;
  const views: Record<string, SidePanes> = {};
  const stored = parsed?.views;
  if (stored && typeof stored === 'object') {
    for (const [view, sides] of Object.entries(stored as Record<string, Partial<SidePanes>>)) {
      if (!registry[view] || !sides || typeof sides !== 'object') continue;
      const left = mergePane(registry, 'left', sides.left);
      const right = mergePane(registry, 'right', sides.right);
      const bottom = mergePane(registry, 'bottom', sides.bottom);
      if (left && right && bottom) views[view] = { left, right, bottom };
    }
  }
  const placed: Record<string, PlacedSlot> = {};
  const storedPlaced = parsed?.placed;
  if (storedPlaced && typeof storedPlaced === 'object') {
    for (const [id, slot] of Object.entries(storedPlaced as Record<string, unknown>)) {
      if (registry[id] && PLACED_SLOTS.includes(slot as PlacedSlot)) placed[id] = slot as PlacedSlot;
    }
  }
  return { layout, views, placed };
}

export function save(storage: StoragePort | null, state: ShellState): boolean {
  if (!storage) return false;
  storage.write(STORAGE_KEY, serialise(state));
  return storage.available;
}

export function saveWorkspace(storage: StoragePort | null, ws: Workspace): boolean {
  if (!storage) return false;
  storage.write(STORAGE_KEY, serialiseWorkspace(ws));
  return storage.available;
}

export function load(storage: StoragePort | null, registry: Registry): ShellState | null {
  if (!storage) return null;
  return deserialise(registry, storage.read(STORAGE_KEY));
}

export function loadWorkspace(storage: StoragePort | null, registry: Registry): Workspace | null {
  if (!storage) return null;
  return deserialiseWorkspace(registry, storage.read(STORAGE_KEY));
}
