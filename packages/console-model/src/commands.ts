/**
 * Commands: everything the palette can do, derived from the placement model.
 *
 * A command carries an *intent* — plain data, never a closure — so the list is
 * testable in Node and the view layer stays the only thing that touches the
 * shell api. The palette is the secondary route to panels and the keyboard
 * twin of the Move control, so its entries follow placement exactly as the
 * nav does: in the centre it is a view, anywhere else it is a panel.
 */

import type { Registry, ShellState, Slot, SurfaceId } from './types.ts';
import { acceptedSlots, isPanelOpen, placementOf, sectionOf } from './placement.ts';
import { SLOT_LABEL } from './surfaces.ts';

export type CommandGroup = 'Views' | 'Panels' | 'Work items' | 'Runs' | 'Actions' | 'Move';

export type Intent =
  | { kind: 'view'; id: SurfaceId }
  | { kind: 'toggle'; id: SurfaceId }
  | { kind: 'reveal'; id: SurfaceId }
  | { kind: 'move'; id: SurfaceId; to: Slot }
  | { kind: 'inspect'; ref: string }
  | { kind: 'theme' }
  | { kind: 'focus' };

export interface Command {
  id: string;
  group: CommandGroup;
  label: string;
  /** Shown right-aligned in mono; searched too. */
  hint?: string;
  /** Searched, never shown. */
  keywords?: string;
  /** Too many to browse: listed only once there is a query. */
  secondary?: boolean;
  intent: Intent;
}

export interface WorkItem { id: string; title: string; meta?: string }
export interface Run { id: string; title: string; surface: SurfaceId }

export interface CommandContext {
  registry: Registry;
  state: ShellState;
  /** The nav's order; the palette lists the same surfaces in the same order. */
  order: readonly SurfaceId[];
  workItems?: readonly WorkItem[];
  runs?: readonly Run[];
  theme: 'dark' | 'light';
  focused: boolean;
}

export function buildCommands(ctx: CommandContext): Command[] {
  const { registry, state, order } = ctx;
  const out: Command[] = [];

  for (const id of order) {
    const spec = registry[id];
    if (!spec) continue;
    if (sectionOf(registry, state, id) === 'primary') {
      const current = state.centre.active === id;
      out.push({ id: `view:${id}`, group: 'Views', label: spec.label,
        hint: current ? 'current view' : 'view', keywords: `go to ${spec.label}`,
        intent: { kind: 'view', id } });
    } else {
      const open = isPanelOpen(state, id);
      out.push({ id: `panel:${id}`, group: 'Panels', label: `${open ? 'Close' : 'Open'} ${spec.label}`,
        hint: 'panel', keywords: `toggle ${spec.label}`, intent: { kind: 'toggle', id } });
    }
  }

  for (const item of ctx.workItems ?? []) {
    out.push({ id: `item:${item.id}`, group: 'Work items', label: item.title, hint: item.id,
      keywords: item.meta, intent: { kind: 'inspect', ref: item.id } });
  }

  for (const run of ctx.runs ?? []) {
    out.push({ id: `run:${run.id}`, group: 'Runs', label: run.title, hint: 'run output',
      intent: { kind: 'reveal', id: run.surface } });
  }

  out.push({ id: 'action:theme', group: 'Actions',
    label: `Switch to ${ctx.theme === 'dark' ? 'light' : 'dark'} theme`, keywords: 'theme appearance',
    intent: { kind: 'theme' } });
  out.push({ id: 'action:focus', group: 'Actions',
    label: ctx.focused ? 'Leave focus mode' : 'Enter focus mode', hint: ctx.focused ? 'Esc' : undefined,
    keywords: 'focus mode collapse panes', intent: { kind: 'focus' } });

  /* Move: every open surface, plus closed nav surfaces, into any slot it
     accepts other than the one it sits in. A closed surface that the nav does
     not list (the inspector) has no "open in" entry: it opens from content. */
  for (const id of Object.keys(registry)) {
    const spec = registry[id];
    const at = placementOf(state, id);
    if (!at && !order.includes(id)) continue;
    for (const slot of acceptedSlots(spec)) {
      if (slot === at) continue;
      out.push({ id: `move:${id}:${slot}`, group: 'Move',
        label: `${at ? 'Move' : 'Open'} ${spec.label} ${at ? 'to' : 'in'} ${SLOT_LABEL[slot]}`,
        hint: slot, secondary: true, intent: { kind: 'move', id, to: slot } });
    }
  }
  return out;
}

const norm = (s: string | undefined) => (s ?? '').toLowerCase().trim();

/**
 * Every token must appear somewhere; a match on the label outranks one on a
 * hint or keyword, and a label that starts with the query outranks a label
 * that merely contains it. Ties keep build order, so views stay above panels.
 */
export function filterCommands(commands: readonly Command[], query: string): Command[] {
  const q = norm(query).replace(/\s+/g, ' ');
  const tokens = q.split(' ').filter(Boolean);
  if (!tokens.length) return commands.filter(c => !c.secondary);
  const scored: Array<{ c: Command; score: number; i: number }> = [];
  commands.forEach((c, i) => {
    const label = norm(c.label);
    const hay = [label, norm(c.hint), norm(c.keywords), norm(c.group)].join(' ');
    if (!tokens.every(t => hay.includes(t))) return;
    const words = label.split(/[^a-z0-9#]+/);
    const score = label.startsWith(q) ? 0
      : tokens.every(t => words.some(w => w.startsWith(t))) ? 1
      : tokens.every(t => label.includes(t)) ? 2 : 3;
    scored.push({ c, score, i });
  });
  scored.sort((a, b) => a.score - b.score || a.i - b.i);
  return scored.map(s => s.c);
}

/** Results keep their groups; groups are ordered by their best match. */
export function groupCommands(commands: readonly Command[]): Array<{ group: CommandGroup; items: Command[] }> {
  const out: Array<{ group: CommandGroup; items: Command[] }> = [];
  for (const c of commands) {
    const existing = out.find(g => g.group === c.group);
    if (existing) existing.items.push(c);
    else out.push({ group: c.group, items: [c] });
  }
  return out;
}

export interface KeyLike { key: string; ctrlKey: boolean; metaKey: boolean; altKey: boolean; shiftKey: boolean }

/** Ctrl+P or ⌘K (and their cross-platform mirrors) toggle the palette. */
export function isPaletteShortcut(e: KeyLike): boolean {
  if (!(e.ctrlKey || e.metaKey) || e.altKey || e.shiftKey) return false;
  const k = String(e.key).toLowerCase();
  return k === 'p' || k === 'k';
}
