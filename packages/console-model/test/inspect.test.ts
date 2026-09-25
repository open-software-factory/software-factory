import { test, describe } from 'node:test';
import assert from 'node:assert/strict';

import { fromPreset, reduce } from '../src/layout.ts';
import { SURFACES } from '../src/surfaces.ts';
import { placementOf } from '../src/placement.ts';
import { inspectMode, inspectPlan, inspectStep, NO_INSPECTION } from '../src/inspect.ts';
import type { ShellState } from '../src/types.ts';

const docked = (): ShellState =>
  fromPreset(SURFACES, {
    centre: { panels: ['floor', 'board', 'runway'], active: 'floor' },
    left: { panels: ['attention'], mode: 'rail' },
    right: { panels: ['inspector'] },
  });
const undocked = (): ShellState => reduce(SURFACES, docked(), { type: 'closeSurface', id: 'inspector' });
const step = (layout: ShellState, inspect = NO_INSPECTION, action: Parameters<typeof inspectStep>[3]) =>
  inspectStep(SURFACES, layout, inspect, action);

describe('the form follows the trigger', () => {
  test('selecting content with nothing docked floats the inspector and leaves the layout alone', () => {
    const before = undocked();
    const { layout, inspect } = step(before, NO_INSPECTION, { type: 'select', ref: 'service-auth#5', via: 'content' });
    assert.equal(layout, before);
    assert.deepEqual(inspect, { ref: 'service-auth#5', floating: true });
    assert.equal(inspectMode(layout, inspect), 'floating');
  });

  test('selecting content while docked keeps it docked and swaps the subject', () => {
    const { layout, inspect } = step(docked(), { ref: 'a#1', floating: false }, { type: 'select', ref: 'b#2', via: 'content' });
    assert.equal(placementOf(layout, 'inspector'), 'right');
    assert.equal(inspect.ref, 'b#2');
    assert.equal(inspectMode(layout, inspect), 'docked');
  });

  test('a command docks directly, at the inspector home slot', () => {
    const { layout, inspect } = step(undocked(), NO_INSPECTION, { type: 'select', ref: 'c#3', via: 'command' });
    assert.equal(placementOf(layout, 'inspector'), 'right');
    assert.equal(inspectMode(layout, inspect), 'docked');
  });
});

describe('pin and unpin', () => {
  test('pin docks a floating inspector through the placement model', () => {
    const floating = step(undocked(), NO_INSPECTION, { type: 'select', ref: 'x#9', via: 'content' });
    const { layout, inspect } = step(floating.layout, floating.inspect, { type: 'pin' });
    assert.equal(placementOf(layout, 'inspector'), 'right');
    assert.equal(layout.right.active, 'inspector');
    assert.deepEqual(inspect, { ref: 'x#9', floating: false });
  });

  test('unpin lifts a docked inspector back out, keeping the subject', () => {
    const { layout, inspect } = step(docked(), { ref: 'x#9', floating: false }, { type: 'unpin' });
    assert.equal(placementOf(layout, 'inspector'), null);
    assert.equal(layout.right.mode, 'hidden');
    assert.equal(inspectMode(layout, inspect), 'floating');
  });

  test('unpin with no subject just closes: there is nothing to float', () => {
    const { layout, inspect } = step(docked(), NO_INSPECTION, { type: 'unpin' });
    assert.equal(placementOf(layout, 'inspector'), null);
    assert.equal(inspectMode(layout, inspect), 'closed');
  });

  test('pin with nothing selected is a no-op; unpin when not docked is a no-op', () => {
    const before = undocked();
    const a = step(before, NO_INSPECTION, { type: 'pin' });
    assert.equal(a.layout, before);
    assert.equal(placementOf(a.layout, 'inspector'), null);
    const b = step(undocked(), { ref: 'x#9', floating: true }, { type: 'unpin' });
    assert.equal(inspectMode(b.layout, b.inspect), 'floating');
  });

  test('a round trip pin → unpin → pin lands in the same place each time', () => {
    let s = step(undocked(), NO_INSPECTION, { type: 'select', ref: 'x#9', via: 'content' });
    s = step(s.layout, s.inspect, { type: 'pin' });
    s = step(s.layout, s.inspect, { type: 'unpin' });
    s = step(s.layout, s.inspect, { type: 'pin' });
    assert.equal(placementOf(s.layout, 'inspector'), 'right');
    assert.equal(inspectMode(s.layout, s.inspect), 'docked');
  });
});

describe('a hidden region is not a docked inspector', () => {
  const hidden = (): ShellState => reduce(SURFACES, docked(), { type: 'toggleRegion', slot: 'right' });

  test('the region toggle hides the pane but leaves the surface placed', () => {
    assert.equal(placementOf(hidden(), 'inspector'), 'right');
    assert.equal(hidden().right.mode, 'hidden');
  });

  test('selecting content then floats rather than activating an invisible pane', () => {
    const { layout, inspect } = step(hidden(), NO_INSPECTION, { type: 'select', ref: 'x#9', via: 'content' });
    assert.equal(inspectMode(layout, inspect), 'floating');
  });

  test('pin reveals the hidden region', () => {
    const floating = step(hidden(), NO_INSPECTION, { type: 'select', ref: 'x#9', via: 'content' });
    const { layout, inspect } = step(floating.layout, floating.inspect, { type: 'pin' });
    assert.equal(layout.right.mode, 'expanded');
    assert.equal(inspectMode(layout, inspect), 'docked');
  });
});

describe('a slot the viewport cannot show is not a docked inspector either', () => {
  test('at the narrow breakpoint the side panes have no width, so a selection floats', () => {
    const narrow = ['left', 'right'] as const;
    assert.equal(inspectMode(docked(), NO_INSPECTION, narrow), 'closed');
    // Without the hint the same selection would have docked into the invisible pane.
    const wide = inspectStep(SURFACES, docked(), NO_INSPECTION, { type: 'select', ref: 'x#9', via: 'content' });
    assert.equal(inspectMode(wide.layout, wide.inspect), 'docked');
    const plan = inspectPlan(docked(), NO_INSPECTION, { type: 'select', ref: 'x#9', via: 'content' }, narrow);
    assert.equal(plan.layoutAction, null);
    assert.equal(inspectMode(docked(), plan.inspect, narrow), 'floating');
  });
});

describe('closing', () => {
  test('dismiss clears the selection and the float', () => {
    const { layout, inspect } = step(undocked(), { ref: 'x#9', floating: true }, { type: 'dismiss' });
    assert.deepEqual(inspect, NO_INSPECTION);
    assert.equal(inspectMode(layout, inspect), 'closed');
  });

  test("closing the docked pane by the pane's own control leaves the selection but nothing showing", () => {
    const layout = reduce(SURFACES, docked(), { type: 'closePane', slot: 'right' });
    const inspect = { ref: 'x#9', floating: false };
    assert.equal(inspectMode(layout, inspect), 'closed');
    const next = step(layout, inspect, { type: 'select', ref: 'y#1', via: 'content' });
    assert.equal(inspectMode(next.layout, next.inspect), 'floating');
  });
});
