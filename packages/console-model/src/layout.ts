/**
 * The layout reducer. Every way the shell can change shape goes through here,
 * so the rules live in one place and drag-and-drop, the Move menu, the nav and
 * the command palette cannot drift apart.
 */

import { SLOTS } from './types.ts';
import type { Action, PaneMode, PaneState, Registry, ShellState, Slot, SurfaceId } from './types.ts';
import { canAccept, placementOf } from './placement.ts';

export const DEFAULT_SIZE: Record<Slot, number> = { centre: 0, left: 288, right: 344, bottom: 132 };

export function emptyPane(slot: Slot): PaneState {
  return {
    panels: [],
    active: null,
    mode: slot === 'centre' ? 'expanded' : slot === 'left' ? 'rail' : 'hidden',
    size: DEFAULT_SIZE[slot],
  };
}

export function emptyState(): ShellState {
  return { centre: emptyPane('centre'), left: emptyPane('left'), right: emptyPane('right'), bottom: emptyPane('bottom') };
}

const set = (state: ShellState, slot: Slot, pane: PaneState): ShellState => ({ ...state, [slot]: pane });

/**
 * Emptying a pane collapses its region rather than leaving a zero-width husk
 * that still claims to be visible.
 *   left   → rail, because the rail is navigation and must survive
 *   right  → hidden, it has no rail
 *   bottom → gone, it is transient by design
 *   centre → simply empty, which is a real state once surfaces can move out
 */
function collapse(slot: Slot, pane: PaneState): PaneState {
  if (pane.panels.length > 0) return pane;
  /* The left region keeps its navigation — that is what the region is for.
     The right has none of its own, so an empty right region is nothing. */
  if (slot === 'left') return pane;
  if (slot === 'right') return { ...pane, mode: 'hidden' };
  return pane;
}

function removeFrom(state: ShellState, slot: Slot, id: SurfaceId): ShellState {
  const pane = state[slot];
  if (!pane.panels.includes(id)) return state;
  const panels = pane.panels.filter(p => p !== id);
  const active = pane.active === id ? (panels[0] ?? null) : pane.active;
  return set(state, slot, collapse(slot, { ...pane, panels, active }));
}

/**
 * Adding to a region also reveals it. A panel that opened into a hidden region
 * would be a silent no-op, which is the worst kind of bug: nothing happens and
 * nothing explains why.
 */
function addTo(state: ShellState, slot: Slot, id: SurfaceId): ShellState {
  const pane = state[slot];
  const panels = pane.panels.includes(id) ? pane.panels : [...pane.panels, id];
  let mode: PaneMode = pane.mode;
  /* Opening reveals a hidden region; it never changes navigation width. The
     operator chose that width, and a panel opening is not a reason to undo it. */
  if (slot === 'left' && mode === 'hidden') mode = 'rail';
  if ((slot === 'right' || slot === 'bottom') && mode === 'hidden') mode = 'expanded';
  return set(state, slot, { ...pane, panels, active: id, mode, size: pane.size || DEFAULT_SIZE[slot] });
}

export function reduce(registry: Registry, state: ShellState, action: Action): ShellState {
  switch (action.type) {
    case 'open': {
      const spec = registry[action.id];
      if (!spec) return state;
      const to = action.to ?? placementOf(state, action.id) ?? spec.home;
      if (!canAccept(spec, to)) return state;
      const at = placementOf(state, action.id);
      // Already there: an activation, not a second copy — but still a reveal.
      // Opening into a region the operator had hidden must show something.
      if (at === to) {
        const pane = state[to];
        const mode = pane.mode !== 'hidden' ? pane.mode : to === 'left' ? 'rail' : 'expanded';
        return set(state, to, { ...pane, active: action.id, mode });
      }
      return addTo(at ? removeFrom(state, at, action.id) : state, to, action.id);
    }

    case 'activate': {
      const pane = state[action.slot];
      if (!pane.panels.includes(action.id)) return state;
      return set(state, action.slot, { ...pane, active: action.id });
    }

    case 'closeSurface': {
      const at = placementOf(state, action.id);
      return at ? removeFrom(state, at, action.id) : state;
    }

    case 'closePane': {
      const pane = state[action.slot];
      // The centre's close affects the current view only; it is never emptied
      // wholesale by one click.
      if (action.slot === 'centre') {
        return pane.active ? removeFrom(state, 'centre', pane.active) : state;
      }
      return set(state, action.slot, collapse(action.slot, { ...pane, panels: [], active: null }));
    }

    case 'toggleSurface': {
      const at = placementOf(state, action.id);
      if (at) return removeFrom(state, at, action.id);
      return reduce(registry, state, { type: 'open', id: action.id });
    }

    case 'move': {
      const spec = registry[action.id];
      if (!spec || !canAccept(spec, action.to)) return state;
      const from = placementOf(state, action.id);
      if (from === action.to) return state;
      return addTo(from ? removeFrom(state, from, action.id) : state, action.to, action.id);
    }

    /*
      Navigation width, and nothing else. An open panel is unaffected: it keeps
      its place beside the navigation at either width. The centre's minimum
      (geometry.ts) is what stops the left region growing too greedy — there is
      no rule here hiding things to make room.
    */
    case 'setNavWide': {
      const pane = state.left;
      if (pane.mode === 'hidden') return state;
      return set(state, 'left', { ...pane, mode: action.wide ? 'nav' : 'rail' });
    }

    case 'setMode': {
      if (action.slot === 'centre') return state;
      return set(state, action.slot, { ...state[action.slot], mode: action.mode });
    }

    case 'toggleRegion': {
      const slot = action.slot;
      const pane = state[slot];
      if (slot === 'bottom') {
        return set(state, slot, pane.panels.length ? { ...pane, panels: [], active: null } : pane);
      }
      if (slot === 'centre') return state;
      if (pane.mode === 'hidden') {
        const mode: PaneMode = slot === 'left' ? 'rail' : 'expanded';
        return set(state, slot, { ...pane, mode });
      }
      /* Expanded navigation is a visible left region; hiding it hides it. */
      return set(state, slot, { ...pane, mode: 'hidden' });
    }

    case 'resize': {
      const pane = state[action.slot];
      return set(state, action.slot, { ...pane, size: Math.max(0, Math.round(action.size)) });
    }

    case 'resizeBy': {
      const pane = state[action.slot];
      const lo = Math.max(0, action.min ?? 0);
      const hi = action.max ?? Number.POSITIVE_INFINITY;
      const size = Math.min(hi, Math.max(lo, Math.round(pane.size + action.delta)));
      return size === pane.size ? state : set(state, action.slot, { ...pane, size });
    }

    case 'restore':
      return action.state;

    default:
      return state;
  }
}

/** Build a starting layout from a plain description — used by presets and tests. */
export function fromPreset(
  registry: Registry,
  preset: Partial<Record<Slot, { panels: SurfaceId[]; active?: SurfaceId; mode?: PaneMode; size?: number }>>,
): ShellState {
  let state = emptyState();
  for (const slot of SLOTS) {
    const spec = preset[slot];
    if (!spec) continue;
    const panels = spec.panels.filter(id => canAccept(registry[id], slot));
    state = {
      ...state,
      [slot]: {
        panels,
        active: spec.active && panels.includes(spec.active) ? spec.active : (panels[0] ?? null),
        mode: spec.mode ?? (slot === 'left' ? 'rail' : panels.length ? 'expanded' : emptyPane(slot).mode),
        size: spec.size ?? DEFAULT_SIZE[slot],
      },
    };
  }
  return state;
}
