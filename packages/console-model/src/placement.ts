/**
 * Placement: where a surface is, where it is allowed to go, and which nav
 * section that implies. This is the single answer to "what kind of thing is
 * this?" — the kind is the placement, not a field on the surface.
 */

import { SLOTS } from './types.ts';
import type { NavSection, Registry, ShellState, Slot, SurfaceId, SurfaceSpec } from './types.ts';

/** Where the surface currently sits, or null when it is closed. */
export function placementOf(state: ShellState, id: SurfaceId): Slot | null {
  return SLOTS.find(slot => state[slot].panels.includes(id)) ?? null;
}

/**
 * A surface with no `accepts` list goes anywhere. That is the current stance
 * on purpose: we do not yet know which placements are genuinely wrong, and a
 * guess baked in now would be harder to remove than to add.
 */
export function canAccept(spec: SurfaceSpec | undefined, slot: Slot): boolean {
  if (!spec) return false;
  return spec.accepts === undefined || spec.accepts.includes(slot);
}

export function acceptedSlots(spec: SurfaceSpec | undefined): Slot[] {
  return SLOTS.filter(slot => canAccept(spec, slot));
}

/**
 * In the centre it is a view; anywhere else it is a panel. A closed surface
 * falls back to its declared home, so shutting something never makes it drift
 * into the other section.
 */
export function sectionOf(registry: Registry, state: ShellState, id: SurfaceId): NavSection {
  const at = placementOf(state, id) ?? registry[id]?.home ?? 'left';
  return at === 'centre' ? 'primary' : 'panel';
}

export function isOpen(state: ShellState, id: SurfaceId): boolean {
  return placementOf(state, id) !== null;
}

/** Open as a panel means open anywhere that is not the centre. */
export function isPanelOpen(state: ShellState, id: SurfaceId): boolean {
  const at = placementOf(state, id);
  return at !== null && at !== 'centre';
}

export function isCurrentView(state: ShellState, id: SurfaceId): boolean {
  return state.centre.active === id;
}

/** Split an ordered id list into the two nav sections, preserving order. */
export function navSections(
  registry: Registry,
  state: ShellState,
  order: readonly SurfaceId[],
): Record<NavSection, SurfaceId[]> {
  const out: Record<NavSection, SurfaceId[]> = { primary: [], panel: [] };
  for (const id of order) out[sectionOf(registry, state, id)].push(id);
  return out;
}
