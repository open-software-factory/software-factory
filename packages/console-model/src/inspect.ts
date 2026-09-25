/**
 * Inspection: what is selected, and whether the inspector floats or is docked.
 *
 * The form follows the trigger (DESIGN.md §5.1.1). Selecting content — a card,
 * a node — must not shove the layout, so the inspector arrives floating; pin
 * docks it through the placement model, unpin lifts it back out. A command
 * (the palette) is operator-requested, so it may dock directly. "Docked" is
 * never stored: it is the inspector surface being open in the layout.
 */

import { reduce } from './layout.ts';
import { placementOf } from './placement.ts';
import type { Action, Registry, ShellState, Slot } from './types.ts';

export const INSPECTOR = 'inspector';

export interface InspectState {
  /** The inspected reference (a work item id), or null. */
  ref: string | null;
  /** A floating inspector is showing. Meaningless while the surface is docked. */
  floating: boolean;
}

export type InspectMode = 'docked' | 'floating' | 'closed';

export type InspectAction =
  | { type: 'select'; ref: string; via: 'content' | 'command' }
  | { type: 'pin' }
  | { type: 'unpin' }
  | { type: 'dismiss' };

export const NO_INSPECTION: InspectState = { ref: null, floating: false };

/**
 * Docked means visible in a pane. A surface left in a hidden region is not
 * showing, and neither is one in a slot the current viewport cannot show
 * (side panes have no width at the narrow breakpoint).
 */
export function isDocked(layout: ShellState, unavailable: readonly Slot[] = []): boolean {
  const at = placementOf(layout, INSPECTOR);
  return at !== null && layout[at].mode !== 'hidden' && !unavailable.includes(at);
}

export function inspectMode(layout: ShellState, inspect: InspectState, unavailable: readonly Slot[] = []): InspectMode {
  if (isDocked(layout, unavailable)) return 'docked';
  return inspect.floating && inspect.ref ? 'floating' : 'closed';
}

const DOCK: Action = { type: 'open', id: INSPECTOR };
const UNDOCK: Action = { type: 'closeSurface', id: INSPECTOR };

/**
 * What an inspection action means: the next inspection state, plus the layout
 * action it implies, if any. The view dispatches that action to the same
 * reducer as every other shape change, so pin and unpin cannot invent
 * placement rules of their own.
 */
export function inspectPlan(
  layout: ShellState,
  inspect: InspectState,
  action: InspectAction,
  unavailable: readonly Slot[] = [],
): { inspect: InspectState; layoutAction: Action | null } {
  switch (action.type) {
    case 'select': {
      if (action.via === 'command' || isDocked(layout, unavailable)) {
        return { inspect: { ref: action.ref, floating: false }, layoutAction: DOCK };
      }
      return { inspect: { ref: action.ref, floating: true }, layoutAction: null };
    }
    case 'pin': {
      if (!inspect.ref) return { inspect, layoutAction: null };
      return { inspect: { ...inspect, floating: false }, layoutAction: DOCK };
    }
    case 'unpin': {
      if (!isDocked(layout, unavailable)) return { inspect, layoutAction: null };
      return { inspect: { ref: inspect.ref, floating: inspect.ref !== null }, layoutAction: UNDOCK };
    }
    case 'dismiss':
      return { inspect: NO_INSPECTION, layoutAction: null };
    default:
      return { inspect, layoutAction: null };
  }
}

/** The plan applied: convenient for tests and for hosts that own both states. */
export function inspectStep(
  registry: Registry,
  layout: ShellState,
  inspect: InspectState,
  action: InspectAction,
): { layout: ShellState; inspect: InspectState } {
  const plan = inspectPlan(layout, inspect, action);
  return {
    layout: plan.layoutAction ? reduce(registry, layout, plan.layoutAction) : layout,
    inspect: plan.inspect,
  };
}
