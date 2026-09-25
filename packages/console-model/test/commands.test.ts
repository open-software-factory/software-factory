import { test, describe } from 'node:test';
import assert from 'node:assert/strict';

import { fromPreset, reduce } from '../src/layout.ts';
import { SURFACES, NAV_ORDER } from '../src/surfaces.ts';
import { buildCommands, filterCommands, groupCommands, isPaletteShortcut } from '../src/commands.ts';
import type { Command, CommandContext } from '../src/commands.ts';
import type { ShellState } from '../src/types.ts';

const start = (): ShellState =>
  fromPreset(SURFACES, {
    centre: { panels: ['floor', 'board', 'runway'], active: 'floor' },
    left: { panels: ['attention'], mode: 'rail' },
    right: { panels: ['inspector'] },
  });

const ctx = (over: Partial<CommandContext> = {}): CommandContext => ({
  registry: SURFACES, state: start(), order: NAV_ORDER, theme: 'dark', focused: false,
  workItems: [
    { id: 'service-auth#5', title: 'Go-live: harden Keycloak availability', meta: 'P0 · Go-Live · bug' },
    { id: 'service-portfolio-api#87', title: 'data-driven token-alias mechanism', meta: 'P1' },
  ],
  runs: [{ id: 'run-4471', title: 'run-4471 · SF-147 · attempt 3', surface: 'logs' }],
  ...over,
});

const byId = (cs: Command[], id: string) => cs.find(c => c.id === id);
const labels = (cs: Command[]) => cs.map(c => c.label);

describe('commands follow placement', () => {
  test('surfaces in the centre are views; the rest are panels with open/close verbs', () => {
    const cs = buildCommands(ctx());
    assert.deepEqual(cs.filter(c => c.group === 'Views').map(c => c.label), ['Floor', 'Board', 'Runway']);
    assert.equal(byId(cs, 'view:floor')?.hint, 'current view');
    assert.deepEqual(labels(cs.filter(c => c.group === 'Panels')),
      ['Close Attention', 'Open Flight recorder', 'Open Run output']);
  });

  test('a view moved out of the centre becomes a panel command', () => {
    const state = reduce(SURFACES, start(), { type: 'move', id: 'runway', to: 'right' });
    const cs = buildCommands(ctx({ state }));
    assert.equal(byId(cs, 'view:runway'), undefined);
    assert.equal(byId(cs, 'panel:runway')?.label, 'Close Runway');
  });

  test('move commands cover every accepted slot except the current one', () => {
    const cs = buildCommands(ctx());
    const attention = cs.filter(c => c.id.startsWith('move:attention:'));
    assert.deepEqual(labels(attention),
      ['Move Attention to Centre', 'Move Attention to Right', 'Move Attention to Bottom']);
    assert.ok(attention.every(c => c.secondary));
  });

  test('a closed nav surface can be opened into a slot; a closed non-nav surface cannot', () => {
    const state = reduce(SURFACES, start(), { type: 'closeSurface', id: 'inspector' });
    const cs = buildCommands(ctx({ state }));
    assert.equal(byId(cs, 'move:logs:bottom')?.label, 'Open Run output in Bottom');
    assert.equal(cs.some(c => c.id.startsWith('move:inspector:')), false);
  });

  test('an open non-nav surface is movable', () => {
    const cs = buildCommands(ctx());
    assert.equal(byId(cs, 'move:inspector:left')?.label, 'Move Inspector to Left');
  });

  test('actions reflect the current theme and focus state', () => {
    assert.equal(byId(buildCommands(ctx()), 'action:theme')?.label, 'Switch to light theme');
    assert.equal(byId(buildCommands(ctx({ theme: 'light' })), 'action:theme')?.label, 'Switch to dark theme');
    assert.equal(byId(buildCommands(ctx({ focused: true })), 'action:focus')?.label, 'Leave focus mode');
  });

  test('work items and runs carry their references as intents', () => {
    const cs = buildCommands(ctx());
    assert.deepEqual(byId(cs, 'item:service-auth#5')?.intent, { kind: 'inspect', ref: 'service-auth#5' });
    assert.deepEqual(byId(cs, 'run:run-4471')?.intent, { kind: 'reveal', id: 'logs' });
  });

  test('every move intent is one the reducer accepts', () => {
    const cs = buildCommands(ctx());
    for (const c of cs.filter(x => x.intent.kind === 'move')) {
      const intent = c.intent as { kind: 'move'; id: string; to: 'centre' | 'left' | 'right' | 'bottom' };
      const next = reduce(SURFACES, start(), { type: 'move', id: intent.id, to: intent.to });
      assert.notEqual(next, start(), `${c.label} changed nothing`);
      assert.ok(next[intent.to].panels.includes(intent.id), `${c.label} did not land`);
    }
  });
});

describe('filtering', () => {
  const cs = buildCommands(ctx());

  test('an empty query lists everything except the secondary move commands', () => {
    const shown = filterCommands(cs, '');
    assert.equal(shown.some(c => c.group === 'Move'), false);
    assert.equal(shown.length, cs.filter(c => !c.secondary).length);
  });

  test('a query surfaces move commands', () => {
    assert.ok(labels(filterCommands(cs, 'attention right')).includes('Move Attention to Right'));
  });

  test('label matches outrank hint and keyword matches', () => {
    const shown = filterCommands(cs, 'run');
    assert.equal(shown[0].label, 'Runway');
    assert.ok(labels(shown).includes('Open Run output'));
    assert.ok(labels(shown).includes('run-4471 · SF-147 · attempt 3'));
  });

  test('every token must match, in any order, case-insensitively', () => {
    assert.deepEqual(labels(filterCommands(cs, 'Keycloak P0')), ['Go-live: harden Keycloak availability']);
    assert.deepEqual(labels(filterCommands(cs, 'nothing like this')), []);
  });

  test('a reference finds its work item', () => {
    assert.equal(filterCommands(cs, '#87')[0].id, 'item:service-portfolio-api#87');
  });

  test('groups are ordered by best match and never split', () => {
    const groups = groupCommands(filterCommands(cs, 'run'));
    assert.equal(groups[0].group, 'Views');
    assert.equal(new Set(groups.map(g => g.group)).size, groups.length);
  });
});

describe('shortcut', () => {
  const key = (k: string, mods: Partial<{ ctrlKey: boolean; metaKey: boolean; altKey: boolean; shiftKey: boolean }> = {}) =>
    ({ key: k, ctrlKey: false, metaKey: false, altKey: false, shiftKey: false, ...mods });

  test('Ctrl+P and ⌘K open it; plain P and Ctrl+Shift+P do not', () => {
    assert.ok(isPaletteShortcut(key('p', { ctrlKey: true })));
    assert.ok(isPaletteShortcut(key('K', { metaKey: true })));
    assert.equal(isPaletteShortcut(key('p')), false);
    assert.equal(isPaletteShortcut(key('P', { ctrlKey: true, shiftKey: true })), false);
    assert.equal(isPaletteShortcut(key('p', { ctrlKey: true, altKey: true })), false);
  });
});
