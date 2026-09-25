import { test, describe } from 'node:test';
import assert from 'node:assert/strict';

import { fromPreset, reduce, emptyState } from '../src/layout.ts';
import { SURFACES } from '../src/surfaces.ts';
import { placementOf } from '../src/placement.ts';
import type { Action, ShellState } from '../src/types.ts';

const start = (): ShellState =>
  fromPreset(SURFACES, {
    centre: { panels: ['floor', 'board', 'runway'], active: 'floor' },
    left: { panels: ['attention'], mode: 'rail' },
    right: { panels: ['inspector'] },
  });

const run = (state: ShellState, ...actions: Action[]) =>
  actions.reduce((s, a) => reduce(SURFACES, s, a), state);

describe('closing', () => {
  test('closing the left pane keeps the rail, because the rail is navigation', () => {
    const next = run(start(), { type: 'closePane', slot: 'left' });
    assert.equal(next.left.panels.length, 0);
    assert.equal(next.left.mode, 'rail', 'left must fall to rail, not hidden');
  });

  test('closing the right pane hides it, because it has no rail', () => {
    const next = run(start(), { type: 'closePane', slot: 'right' });
    assert.equal(next.right.mode, 'hidden');
  });

  test('closing the last panel collapses the region instead of leaving a husk', () => {
    const next = run(start(), { type: 'closeSurface', id: 'inspector' });
    assert.equal(next.right.panels.length, 0);
    assert.equal(next.right.mode, 'hidden', 'an empty pane must not still claim to be visible');
  });

  test('the centre close affects only the current view', () => {
    const next = run(start(), { type: 'closePane', slot: 'centre' });
    assert.deepEqual(next.centre.panels, ['board', 'runway']);
    assert.equal(next.centre.active, 'board', 'a neighbour takes over as current');
  });

  test('only the region toggle removes a region entirely', () => {
    const closed = run(start(), { type: 'closePane', slot: 'left' });
    assert.equal(closed.left.mode, 'rail');
    const hidden = run(closed, { type: 'toggleRegion', slot: 'left' });
    assert.equal(hidden.left.mode, 'hidden');
    const back = run(hidden, { type: 'toggleRegion', slot: 'left' });
    assert.equal(back.left.mode, 'rail', 'restoring an empty left region gives back the rail');
  });
});

describe('opening', () => {
  test('a surface opens at its declared home', () => {
    const next = run(emptyState(), { type: 'open', id: 'recorder' });
    assert.equal(placementOf(next, 'recorder'), 'bottom');
  });

  test('opening into a hidden region reveals it', () => {
    const hidden = run(start(), { type: 'closePane', slot: 'right' });
    assert.equal(hidden.right.mode, 'hidden');
    const next = run(hidden, { type: 'open', id: 'logs' });
    assert.equal(next.right.mode, 'expanded', 'opening must never be a silent no-op');
    assert.equal(next.right.active, 'logs');
  });

  test('opening into the bottom region marks it shown, so a toggle can read its state', () => {
    const next = run(start(), { type: 'open', id: 'recorder' });
    assert.equal(placementOf(next, 'recorder'), 'bottom');
    assert.equal(next.bottom.mode, 'expanded', 'a region with panels must not report itself hidden');
  });

  test('opening a surface that already sits in a hidden region also reveals it', () => {
    const hidden = run(start(), { type: 'toggleRegion', slot: 'right' });
    assert.equal(placementOf(hidden, 'inspector'), 'right');
    assert.equal(hidden.right.mode, 'hidden');
    const next = run(hidden, { type: 'open', id: 'inspector' });
    assert.equal(next.right.mode, 'expanded', 'an activation into a hidden region is still a no-op to the eye');
    assert.equal(next.right.panels.filter(p => p === 'inspector').length, 1, 'no second copy');
  });

  test('opening something already open activates it rather than duplicating it', () => {
    const next = run(start(), { type: 'open', id: 'runway' });
    assert.deepEqual(next.centre.panels, ['floor', 'board', 'runway']);
    assert.equal(next.centre.active, 'runway');
  });

  test('toggling is symmetric', () => {
    const on = run(start(), { type: 'toggleSurface', id: 'recorder' });
    assert.equal(placementOf(on, 'recorder'), 'bottom');
    const off = run(on, { type: 'toggleSurface', id: 'recorder' });
    assert.equal(placementOf(off, 'recorder'), null);
  });

  test('opening a panel never changes which view is current', () => {
    const next = run(start(), { type: 'toggleSurface', id: 'recorder' }, { type: 'toggleSurface', id: 'logs' });
    assert.equal(next.centre.active, 'floor');
  });
});

describe('moving', () => {
  test('a surface exists in exactly one slot after a move', () => {
    const next = run(start(), { type: 'move', id: 'runway', to: 'right' });
    assert.equal(placementOf(next, 'runway'), 'right');
    assert.ok(!next.centre.panels.includes('runway'));
  });

  test('moving the current view hands current to a neighbour', () => {
    const next = run(start(), { type: 'move', id: 'floor', to: 'bottom' });
    assert.equal(next.centre.active, 'board');
  });

  test('moving the last centre surface out leaves an empty centre, not a broken one', () => {
    const next = run(
      start(),
      { type: 'move', id: 'floor', to: 'left' },
      { type: 'move', id: 'board', to: 'left' },
      { type: 'move', id: 'runway', to: 'left' },
    );
    assert.deepEqual(next.centre.panels, []);
    assert.equal(next.centre.active, null);
  });

  test('moving to where it already is does nothing', () => {
    const state = start();
    assert.strictEqual(run(state, { type: 'move', id: 'floor', to: 'centre' }), state);
  });
});

describe('navigation is a width, not a surface', () => {
  test('there is no nav surface to open', () => {
    assert.equal(SURFACES.nav, undefined, 'navigation must never be placeable as a panel');
  });

  /*
    Mode is navigation width; `panels` is content. Nothing in one may quietly
    change the other. Two controls that looked identical, and a nav that
    reported a panel as open while nothing was on screen, both came from a
    mode that could hide an open panel.
  */
  test('widening navigation leaves an open panel exactly where it was', () => {
    const next = run(start(), { type: 'setNavWide', wide: true });
    assert.equal(next.left.mode, 'nav');
    assert.deepEqual(next.left.panels, ['attention'], 'still open, and still shown');
  });

  test('opening a panel does not change navigation width', () => {
    const wide = { ...start(), left: { ...start().left, panels: [], mode: 'nav' as const } };
    const next = run(wide, { type: 'open', id: 'attention', to: 'left' });
    assert.equal(next.left.mode, 'nav', 'the operator chose that width');
  });

  test('closing a panel does not change navigation width', () => {
    const wide = fromPreset(SURFACES, { left: { panels: ['attention'], mode: 'nav' } });
    const next = run(wide, { type: 'closeSurface', id: 'attention' });
    assert.equal(next.left.mode, 'nav');
    assert.deepEqual(next.left.panels, []);
  });

  test('there is exactly one way to say a panel is not showing', () => {
    const closed = run(start(), { type: 'closeSurface', id: 'attention' });
    assert.deepEqual(closed.left.panels, [], 'closing removes it');
    const narrowed = run(start(), { type: 'setNavWide', wide: false });
    assert.deepEqual(narrowed.left.panels, ['attention'], 'no width hides it');
  });

  test('a hidden left region is not silently revealed by a width change', () => {
    const hidden = run(start(), { type: 'toggleRegion', slot: 'left' });
    assert.strictEqual(run(hidden, { type: 'setNavWide', wide: true }), hidden);
  });

  test('expanded navigation still hides with the region toggle', () => {
    const wide = { ...start(), left: { ...start().left, panels: [], mode: 'nav' as const } };
    assert.equal(run(wide, { type: 'toggleRegion', slot: 'left' }).left.mode, 'hidden');
  });
});

describe('resize by a step', () => {
  test('two steps add up even when dispatched back to back', () => {
    const a = run(start(), { type: 'resizeBy', slot: 'right', delta: 16 });
    const b = run(a, { type: 'resizeBy', slot: 'right', delta: 16 });
    assert.equal(b.right.size, start().right.size + 32);
  });

  test('bounds hold and a no-op returns the same state', () => {
    const s = run(start(), { type: 'resizeBy', slot: 'bottom', delta: 9999, min: 80, max: 420 });
    assert.equal(s.bottom.size, 420);
    assert.equal(run(s, { type: 'resizeBy', slot: 'bottom', delta: 1, min: 80, max: 420 }), s);
  });
});

describe('resize', () => {
  test('a size is stored per pane and never negative', () => {
    const next = run(start(), { type: 'resize', slot: 'left', size: -40 });
    assert.equal(next.left.size, 0);
  });
});
