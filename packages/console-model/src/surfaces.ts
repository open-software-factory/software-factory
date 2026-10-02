/**
 * The surface registry: everything the shell needs to place a surface, and
 * nothing about how it draws. Rendering is bound by id in the view layer, so
 * this file stays testable in plain Node.
 *
 * `accepts` is deliberately permissive today — every surface takes every slot.
 * That is the current decision, not an oversight: we do not yet know which
 * placements are genuinely wrong, and the way to find out is to build the
 * views and try. Tighten a list only when a real case proves it.
 */

import { SLOTS } from './types.ts';
import type { Registry, Slot, SurfaceSpec } from './types.ts';

const ANYWHERE: readonly Slot[] = SLOTS;

const spec = (s: Omit<SurfaceSpec, 'accepts'> & { accepts?: readonly Slot[] }): SurfaceSpec => ({
  accepts: ANYWHERE,
  ...s,
});

/* Width appetites in CSS px at the shell's UI scale (125%, see geometry.ts). */
export const SURFACES: Registry = Object.freeze({
  floor: spec({ id: 'floor', label: 'Floor', glyph: '◈', min: 525, max: 5000, home: 'centre' }),
  board: spec({ id: 'board', label: 'Board', glyph: '▤', min: 650, max: 5000, home: 'centre' }),
  runway: spec({ id: 'runway', label: 'Runway', glyph: '↗', min: 575, max: 5000, home: 'centre' }),
  /* There is no `nav` surface. Navigation is the rail, at one of two widths
     (see PaneMode 'rail' and 'nav'). Making it a surface let it be opened as
     a tab beside itself, which is not a thing. */
  attention: spec({ id: 'attention', label: 'Attention', glyph: '◉', min: 300, max: 575, home: 'left' }),
  inspector: spec({ id: 'inspector', label: 'Inspector', glyph: '▣', min: 375, max: 700, home: 'right' }),
  logs: spec({ id: 'logs', label: 'Run output', glyph: '⌇', min: 400, max: 875, home: 'right' }),
  recorder: spec({ id: 'recorder', label: 'Flight recorder', glyph: '⏱', min: 0, max: 5000, home: 'bottom' }),
});

/** The order the nav lists surfaces in, before it splits them into sections. */
export const NAV_ORDER: readonly string[] = ['floor', 'board', 'runway', 'attention', 'recorder', 'logs'];

export const SLOT_LABEL: Record<Slot, string> = {
  centre: 'Centre',
  left: 'Left',
  right: 'Right',
  bottom: 'Bottom',
};
