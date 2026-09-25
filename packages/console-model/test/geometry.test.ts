import { test, describe } from 'node:test';
import assert from 'node:assert/strict';

import { fromPreset } from '../src/layout.ts';
import { SURFACES } from '../src/surfaces.ts';
import {
  breakpointFor, CENTRE_MIN, fitToViewport, NAV_WIDTH, RAIL_COMPACT, RAIL_GAP, RAIL_WIDTH, railWidthFor, sideWidth,
} from '../src/geometry.ts';

// Sizes inside the panels' appetites, so the clamp leaves them alone.
const LEFT = 360, RIGHT = 430;
const start = () =>
  fromPreset(SURFACES, {
    left: { panels: ['attention'], mode: 'rail', size: LEFT },
    right: { panels: ['inspector'], size: RIGHT },
  });

describe('breakpoints', () => {
  test('boundaries are inclusive at the lower edge', () => {
    assert.equal(breakpointFor(1600), 'xl');
    assert.equal(breakpointFor(1599), 'lg');
    assert.equal(breakpointFor(1280), 'lg');
    assert.equal(breakpointFor(1279), 'md');
    assert.equal(breakpointFor(960), 'md');
    assert.equal(breakpointFor(959), 'sm');
  });
});

describe('rail width', () => {
  test('the compact rail is mouse-only, because it trades the label for a tooltip', () => {
    assert.equal(railWidthFor('md', false), RAIL_COMPACT);
    assert.equal(railWidthFor('md', true), RAIL_WIDTH, 'touch gets the drawer, never a smaller target');
  });

  test('wider breakpoints keep the labelled rail', () => {
    assert.equal(railWidthFor('lg', false), RAIL_WIDTH);
    assert.equal(railWidthFor('xl', false), RAIL_WIDTH);
  });
});

describe('side width', () => {
  test('navigation alone is its width plus the gap, at either size', () => {
    const rail = { ...start(), left: { ...start().left, panels: [], mode: 'rail' as const } };
    assert.equal(sideWidth(SURFACES, rail, 'left', 'lg', RAIL_WIDTH), RAIL_WIDTH + RAIL_GAP);
    const wide = { ...start(), left: { ...start().left, panels: [], mode: 'nav' as const } };
    assert.equal(sideWidth(SURFACES, wide, 'left', 'lg', RAIL_WIDTH), NAV_WIDTH + RAIL_GAP);
  });

  /* Width and content are independent: a panel adds its body to whichever
     navigation width is showing, and neither hides the other. */
  test('a panel adds its body to the rail', () => {
    assert.equal(sideWidth(SURFACES, start(), 'left', 'lg', RAIL_WIDTH), RAIL_WIDTH + RAIL_GAP + LEFT);
  });

  test('a panel adds its body to expanded navigation too', () => {
    const wide = { ...start(), left: { ...start().left, mode: 'nav' as const } };
    assert.equal(sideWidth(SURFACES, wide, 'left', 'lg', RAIL_WIDTH), NAV_WIDTH + RAIL_GAP + LEFT);
  });

  test('a hidden region is zero', () => {
    const state = { ...start(), right: { ...start().right, mode: 'hidden' as const } };
    assert.equal(sideWidth(SURFACES, state, 'right', 'lg', RAIL_WIDTH), 0);
  });

  test('content minimum wins over a too-small stored size', () => {
    const state = fromPreset(SURFACES, { right: { panels: ['inspector'], size: 120 } });
    assert.equal(sideWidth(SURFACES, state, 'right', 'lg', RAIL_WIDTH), SURFACES.inspector.min);
  });

  test('content maximum wins over a too-large stored size', () => {
    const state = fromPreset(SURFACES, { right: { panels: ['inspector'], size: 9000 } });
    assert.equal(sideWidth(SURFACES, state, 'right', 'lg', RAIL_WIDTH), SURFACES.inspector.max);
  });

  test('two panels in one pane negotiate to the strictest bounds', () => {
    const state = fromPreset(SURFACES, { right: { panels: ['inspector', 'logs'], size: 200 } });
    // strictest min is the larger of the two: the logs' appetite beats the inspector's
    assert.equal(sideWidth(SURFACES, state, 'right', 'lg', RAIL_WIDTH), Math.max(SURFACES.inspector.min, SURFACES.logs.min));
  });

  test('the narrowest breakpoint gives every side region to the centre', () => {
    assert.equal(sideWidth(SURFACES, start(), 'left', 'sm', RAIL_WIDTH), 0);
    assert.equal(sideWidth(SURFACES, start(), 'right', 'sm', RAIL_WIDTH), 0);
  });
});

describe('the centre floor', () => {
  test('panes may be generous when there is room', () => {
    assert.deepEqual(fitToViewport(3440, 600, 700), { left: 600, right: 700 });
  });

  test('panes give way rather than crushing the centre', () => {
    const fitted = fitToViewport(1280, 500, 500);
    assert.ok(fitted.left + fitted.right + CENTRE_MIN <= 1280);
    assert.ok(fitted.left > 0 && fitted.right > 0, 'both shrink proportionally, neither vanishes');
  });

  test('an exact fit is left alone', () => {
    assert.deepEqual(fitToViewport(CENTRE_MIN + 400, 200, 200), { left: 200, right: 200 });
  });
});
