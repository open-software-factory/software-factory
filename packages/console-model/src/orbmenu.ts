/**
 * What the orb's ring offers, derived from the shell the same way the palette
 * is: from the placement model, the nav order, and what is selected.
 *
 * Entries carry intents (plain data), never closures, so the ring is testable
 * in Node and the view layer stays the only thing that touches the shell api.
 * When the navigation is not on screen — focus mode, or the smallest size,
 * where the nav is a drawer — the orb *is* the navigation: the views sit on
 * the ring itself instead of under "Go to", and the panels get their own
 * submenu. The ring never hides the nav itself: that is a mode with its own
 * visible exit, not a menu entry.
 */

import type { Registry, ShellState, SurfaceId } from './types.ts';
import type { Intent } from './commands.ts';
import { isPanelOpen, sectionOf } from './placement.ts';

export type OrbIntent =
  | Intent
  | { kind: 'palette' }
  | { kind: 'assistant'; mode: 'ask' | 'voice'; about?: string };

export interface OrbEntry {
  id: string;
  label: string;
  glyph: string;
  intent?: OrbIntent;
  children?: OrbEntry[];
  /** The view the operator is already in. */
  current?: boolean;
}

export interface OrbSelection { ref: string; title?: string }

export interface OrbContext {
  registry: Registry;
  state: ShellState;
  order: readonly SurfaceId[];
  /** What the inspector is on, if anything. */
  selection?: OrbSelection | null;
  /** The navigation is not on screen: focus mode, or the smallest size where it is a drawer. */
  navHidden: boolean;
}

export function orbMenu(ctx: OrbContext): OrbEntry[] {
  const { registry, state, order } = ctx;
  const views: OrbEntry[] = [];
  const panels: OrbEntry[] = [];
  for (const id of order) {
    const spec = registry[id];
    if (!spec) continue;
    if (sectionOf(registry, state, id) === 'primary') {
      views.push({ id: `view:${id}`, label: spec.label, glyph: spec.glyph, current: state.centre.active === id, intent: { kind: 'view', id } });
    } else {
      const open = isPanelOpen(state, id);
      panels.push({ id: `panel:${id}`, label: `${open ? 'Close' : 'Open'} ${spec.label}`, glyph: spec.glyph, intent: { kind: 'toggle', id } });
    }
  }

  const out: OrbEntry[] = [
    { id: 'ask', label: 'Ask', glyph: '?', intent: { kind: 'assistant', mode: 'ask' } },
    { id: 'voice', label: 'Voice', glyph: '◉', intent: { kind: 'assistant', mode: 'voice' } },
    { id: 'commands', label: 'Commands', glyph: '⌘', intent: { kind: 'palette' } },
  ];

  if (ctx.navHidden) {
    out.push(...views);
    out.push({ id: 'panels', label: 'Panels', glyph: '▦', children: panels });
  } else {
    out.push({ id: 'go', label: 'Go to', glyph: '→', children: views });
  }

  // Context: the selected item first, then the view the operator is in.
  const here: OrbEntry[] = [];
  const sel = ctx.selection;
  if (sel) {
    here.push({ id: `inspect:${sel.ref}`, label: `Inspect ${sel.ref}`, glyph: '▣', intent: { kind: 'inspect', ref: sel.ref } });
    here.push({ id: `about:${sel.ref}`, label: `Ask about ${sel.ref}`, glyph: '?', intent: { kind: 'assistant', mode: 'ask', about: sel.ref } });
  }
  const active = state.centre.active;
  const view = active ? registry[active] : undefined;
  if (view) here.push({ id: `about-view:${view.id}`, label: `Ask about ${view.label}`, glyph: view.glyph, intent: { kind: 'assistant', mode: 'ask', about: view.label } });
  if (here.length) out.push({ id: 'here', label: sel ? sel.ref : 'Here', glyph: '◎', children: here });

  return out;
}

/** The intent behind an entry id, at any depth; undefined for a submenu or an unknown id. */
export function orbIntentOf(entries: readonly OrbEntry[], id: string): OrbIntent | undefined {
  for (const e of entries) {
    if (e.id === id) return e.intent;
    if (e.children) { const found = orbIntentOf(e.children, id); if (found) return found; }
  }
  return undefined;
}
