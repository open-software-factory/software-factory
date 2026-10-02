import { test, describe } from 'node:test';
import assert from 'node:assert/strict';

import { fromPreset, reduce } from '../src/layout.ts';
import { SURFACES, NAV_ORDER } from '../src/surfaces.ts';
import { acceptedSlots, canAccept, navSections, sectionOf } from '../src/placement.ts';
import type { Registry, ShellState, SurfaceSpec } from '../src/types.ts';

const start = (): ShellState =>
  fromPreset(SURFACES, {
    centre: { panels: ['floor', 'board', 'runway'], active: 'floor' },
    left: { panels: ['attention'], mode: 'rail' },
    right: { panels: ['inspector'] },
  });

describe('section follows placement', () => {
  test('in the centre it is a view', () => {
    assert.equal(sectionOf(SURFACES, start(), 'floor'), 'primary');
  });

  test('anywhere else it is a panel', () => {
    assert.equal(sectionOf(SURFACES, start(), 'attention'), 'panel');
  });

  test('a view moved out of the centre becomes a panel', () => {
    const next = reduce(SURFACES, start(), { type: 'move', id: 'runway', to: 'right' });
    assert.equal(sectionOf(SURFACES, next, 'runway'), 'panel');
  });

  test('a panel moved into the centre becomes a view', () => {
    const next = reduce(SURFACES, start(), { type: 'move', id: 'attention', to: 'centre' });
    assert.equal(sectionOf(SURFACES, next, 'attention'), 'primary');
  });

  test('a closed surface returns to its home section rather than drifting', () => {
    const moved = reduce(SURFACES, start(), { type: 'move', id: 'attention', to: 'centre' });
    const closed = reduce(SURFACES, moved, { type: 'closeSurface', id: 'attention' });
    assert.equal(sectionOf(SURFACES, closed, 'attention'), 'panel');
  });

  test('nav sections keep the declared order within each group', () => {
    const groups = navSections(SURFACES, start(), NAV_ORDER);
    assert.deepEqual(groups.primary, ['floor', 'board', 'runway']);
    assert.deepEqual(groups.panel, ['attention', 'recorder', 'logs']);
  });

  test('every surface lands in exactly one section', () => {
    const groups = navSections(SURFACES, start(), NAV_ORDER);
    assert.equal(groups.primary.length + groups.panel.length, NAV_ORDER.length);
  });
});

describe('slot acceptance', () => {
  test('an omitted accepts list means anywhere — the current permissive stance', () => {
    const loose = { id: 'x', label: 'X', glyph: '·', min: 0, max: 100, canRail: true, home: 'left' } as SurfaceSpec;
    assert.deepEqual(acceptedSlots(loose), ['centre', 'left', 'right', 'bottom']);
  });

  test('a declared list is honoured', () => {
    const picky: SurfaceSpec = {
      id: 'picky', label: 'Picky', glyph: '·', min: 0, max: 100, canRail: false,
      home: 'bottom', accepts: ['bottom'],
    };
    assert.ok(canAccept(picky, 'bottom'));
    assert.ok(!canAccept(picky, 'left'));
  });

  test('a move into a refused slot is rejected, leaving the layout untouched', () => {
    const registry: Registry = {
      ...SURFACES,
      recorder: { ...SURFACES.recorder, accepts: ['bottom'] },
    };
    const state = fromPreset(registry, { bottom: { panels: ['recorder'] } });
    const next = reduce(registry, state, { type: 'move', id: 'recorder', to: 'left' });
    assert.strictEqual(next, state);
  });

  test('a preset that places a surface somewhere it refuses drops that placement', () => {
    const registry: Registry = {
      ...SURFACES,
      inspector: { ...SURFACES.inspector, accepts: ['right'] },
    };
    const state = fromPreset(registry, { centre: { panels: ['inspector', 'floor'] } });
    assert.deepEqual(state.centre.panels, ['floor']);
  });

  test('today every real surface accepts every slot', () => {
    for (const id of NAV_ORDER) {
      assert.deepEqual(acceptedSlots(SURFACES[id]), ['centre', 'left', 'right', 'bottom'], id);
    }
  });
});
