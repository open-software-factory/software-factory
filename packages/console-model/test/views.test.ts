import { test, describe } from 'node:test';
import assert from 'node:assert/strict';

import { fromPreset } from '../src/layout.ts';
import { SURFACES } from '../src/surfaces.ts';
import { placementOf } from '../src/placement.ts';
import { reduceWorkspace } from '../src/views.ts';
import type { Workspace } from '../src/views.ts';
import { deserialiseWorkspace, serialiseWorkspace } from '../src/persist.ts';
import type { Action } from '../src/types.ts';

const start = (): Workspace => ({
  layout: fromPreset(SURFACES, {
    centre: { panels: ['floor', 'board', 'runway'], active: 'floor' },
    left: { panels: ['attention'], mode: 'rail' },
    right: { panels: ['inspector'] },
  }),
  views: {},
  placed: {},
});
const run = (ws: Workspace, ...actions: Action[]) => actions.reduce((s, a) => reduceWorkspace(SURFACES, s, a), ws);
const rightOf = (ws: Workspace) => ws.layout.right.panels;

describe('each view remembers its panes', () => {
  test('a first visit keeps whatever is open', () => {
    const ws = run(start(), { type: 'activate', slot: 'centre', id: 'board' });
    assert.deepEqual(rightOf(ws), ['inspector']);
    assert.deepEqual(Object.keys(ws.views), ['floor'], 'the view left behind is remembered');
  });

  test('leaving stores, returning restores', () => {
    let ws = run(start(), { type: 'activate', slot: 'centre', id: 'board' });
    ws = run(ws, { type: 'open', id: 'logs' });                          // Board gets logs on the right
    assert.deepEqual(rightOf(ws), ['inspector', 'logs']);
    ws = run(ws, { type: 'activate', slot: 'centre', id: 'floor' });    // Floor never had logs
    assert.deepEqual(rightOf(ws), ['inspector']);
    ws = run(ws, { type: 'activate', slot: 'centre', id: 'board' });    // Board gets them back
    assert.deepEqual(rightOf(ws), ['inspector', 'logs']);
    assert.equal(ws.layout.right.active, 'logs');
  });

  test('closing a side panel on one view does not close it on another', () => {
    let ws = run(start(), { type: 'activate', slot: 'centre', id: 'board' }, { type: 'closePane', slot: 'right' });
    assert.deepEqual(rightOf(ws), []);
    ws = run(ws, { type: 'activate', slot: 'centre', id: 'floor' });
    assert.deepEqual(rightOf(ws), ['inspector'], 'Floor still has its inspector');
  });

  test('a surface moved into the centre is not resurrected beside it by a recall', () => {
    let ws = run(start(), { type: 'activate', slot: 'centre', id: 'board' });
    // Board is remembered with the inspector on the right (stored when we leave).
    ws = run(ws, { type: 'activate', slot: 'centre', id: 'floor' });
    ws = run(ws, { type: 'move', id: 'inspector', to: 'centre' });      // inspector is now a view
    ws = run(ws, { type: 'activate', slot: 'centre', id: 'board' });
    assert.equal(placementOf(ws.layout, 'inspector'), 'centre');
    assert.deepEqual(rightOf(ws), [], 'not both in the centre and on the right');
  });

  test('move and restore change the view on purpose and skip the recall', () => {
    let ws = run(start(), { type: 'activate', slot: 'centre', id: 'board' }, { type: 'open', id: 'logs' });
    ws = run(ws, { type: 'activate', slot: 'centre', id: 'floor' });
    // Moving runway out makes board active; the right pane must stay as the move left it.
    ws = run(ws, { type: 'move', id: 'floor', to: 'right' });
    assert.equal(ws.layout.centre.active, 'board');
    assert.deepEqual(rightOf(ws), ['inspector', 'floor']);
  });

  test('widths are global: a resize on one view carries to every view', () => {
    let ws = run(start(), { type: 'activate', slot: 'centre', id: 'board' });
    ws = run(ws, { type: 'resize', slot: 'right', size: 420 });
    ws = run(ws, { type: 'activate', slot: 'centre', id: 'floor' });   // Floor remembered with the old width
    assert.equal(ws.layout.right.size, 420, 'the seam stays where the operator left it');
    ws = run(ws, { type: 'activate', slot: 'centre', id: 'board' });
    assert.equal(ws.layout.right.size, 420);
  });

  test('navigation width is global too', () => {
    let ws = run(start(), { type: 'activate', slot: 'centre', id: 'board' });
    ws = run(ws, { type: 'setNavWide', wide: true });
    ws = run(ws, { type: 'activate', slot: 'centre', id: 'floor' });
    assert.equal(ws.layout.left.mode, 'nav');
  });

  test('a remembered panel reveals a region the operator had hidden', () => {
    let ws = run(start(), { type: 'activate', slot: 'centre', id: 'board' }, { type: 'open', id: 'logs' });
    ws = run(ws, { type: 'activate', slot: 'centre', id: 'floor' });
    ws = run(ws, { type: 'toggleRegion', slot: 'right' });              // hide it on Floor
    assert.equal(ws.layout.right.mode, 'hidden');
    ws = run(ws, { type: 'activate', slot: 'centre', id: 'board' });   // Board wants logs showing
    assert.equal(ws.layout.right.mode, 'expanded');
    assert.deepEqual(rightOf(ws), ['inspector', 'logs']);
  });

  test('an unchanged layout returns the same workspace', () => {
    const ws = start();
    assert.equal(run(ws, { type: 'activate', slot: 'centre', id: 'nope' }), ws);
  });
});

describe('per-view memory persists with the layout', () => {
  test('round trip keeps every remembered view', () => {
    let ws = run(start(), { type: 'activate', slot: 'centre', id: 'board' }, { type: 'open', id: 'logs' });
    ws = run(ws, { type: 'activate', slot: 'centre', id: 'floor' });
    const back = deserialiseWorkspace(SURFACES, serialiseWorkspace(ws));
    assert.deepEqual(back?.views, ws.views);
    assert.deepEqual(back?.layout, ws.layout);
  });

  test('a remembered view that no longer exists is dropped; bad panels inside a memory are dropped', () => {
    const raw = JSON.stringify({ version: 1, layout: start().layout, views: {
      ghost: { left: { panels: [] }, right: { panels: [] }, bottom: { panels: [] } },
      board: { left: { panels: ['attention'] }, right: { panels: ['logs', 'nope'] }, bottom: { panels: [] } },
    } });
    const back = deserialiseWorkspace(SURFACES, raw);
    assert.deepEqual(Object.keys(back!.views), ['board']);
    assert.deepEqual(back!.views.board.right.panels, ['logs']);
  });

  test('an old payload without views still loads', () => {
    const raw = JSON.stringify({ version: 1, layout: start().layout });
    assert.deepEqual(deserialiseWorkspace(SURFACES, raw)?.views, {});
  });
});

describe('a surface the operator places stays where it was put', () => {
  test('report A: moved from the centre to the right, it stays on the right across a view switch', () => {
    let ws = run(start(),
      { type: 'activate', slot: 'centre', id: 'board' },
      { type: 'activate', slot: 'centre', id: 'runway' },
      { type: 'activate', slot: 'centre', id: 'floor' },
    ); // Board and Runway each now remember right: [inspector]
    ws = run(ws, { type: 'move', id: 'board', to: 'right' }); // drag Board into the right pane, from Floor
    assert.deepEqual(rightOf(ws), ['inspector', 'board']);
    ws = run(ws, { type: 'activate', slot: 'centre', id: 'runway' }); // pick another view in the nav
    assert.ok(rightOf(ws).includes('board'), 'the moved surface stays in the right pane');
    assert.notEqual(ws.layout.right.mode, 'hidden', 'the right region is not hidden');
  });

  test('report A: moved from the centre to the left, it stays on the left and the left pane is revealed', () => {
    let ws = run(start(),
      { type: 'activate', slot: 'centre', id: 'board' },
      { type: 'activate', slot: 'centre', id: 'runway' },
      { type: 'activate', slot: 'centre', id: 'floor' },
    );
    ws = run(ws, { type: 'move', id: 'runway', to: 'left' }); // drag Runway into the left pane, from Floor
    ws = run(ws, { type: 'toggleRegion', slot: 'left' }); // the operator later hides the whole left region
    assert.equal(ws.layout.left.mode, 'hidden');
    ws = run(ws, { type: 'activate', slot: 'centre', id: 'board' }); // switch views
    assert.ok(ws.layout.left.panels.includes('runway'), 'the moved surface stays in the left pane');
    assert.notEqual(ws.layout.left.mode, 'hidden', 'a left pane holding a panel is not left hidden');
  });

  test('report B: moved out of a pane a view remembers it in, it does not come back there', () => {
    let ws = run(start(), { type: 'activate', slot: 'centre', id: 'board' }, { type: 'open', id: 'logs' });
    // Board now remembers right: [inspector, logs].
    ws = run(ws, { type: 'activate', slot: 'centre', id: 'runway' }); // Runway inherits it, unvisited so far
    ws = run(ws, { type: 'move', id: 'logs', to: 'left' }); // drag the run output surface to the left, from Runway
    ws = run(ws, { type: 'activate', slot: 'centre', id: 'board' }); // back to the view that remembers it on the right
    assert.ok(ws.layout.left.panels.includes('logs'), 'logs shows in the left pane');
    assert.ok(!rightOf(ws).includes('logs'), 'and not in the right pane, where Board last had it');
  });

  test('closing a placed surface clears the placement; it does not come back on a later switch', () => {
    let ws = run(start(),
      { type: 'activate', slot: 'centre', id: 'board' },
      { type: 'activate', slot: 'centre', id: 'runway' },
      { type: 'activate', slot: 'centre', id: 'floor' },
    );
    ws = run(ws, { type: 'move', id: 'runway', to: 'right' });
    assert.ok(rightOf(ws).includes('runway'));
    ws = run(ws, { type: 'closeSurface', id: 'runway' });
    assert.equal(placementOf(ws.layout, 'runway'), null);
    ws = run(ws, { type: 'activate', slot: 'centre', id: 'board' });
    assert.equal(placementOf(ws.layout, 'runway'), null, 'closing it stays closed after a view switch');
  });

  test('moving a placed surface back to the centre clears the placement', () => {
    let ws = run(start(),
      { type: 'activate', slot: 'centre', id: 'board' },
      { type: 'activate', slot: 'centre', id: 'runway' },
      { type: 'activate', slot: 'centre', id: 'floor' },
    );
    ws = run(ws, { type: 'move', id: 'runway', to: 'right' });
    ws = run(ws, { type: 'move', id: 'runway', to: 'centre' });
    assert.equal(placementOf(ws.layout, 'runway'), 'centre');
    ws = run(ws, { type: 'activate', slot: 'centre', id: 'board' });
    assert.ok(!rightOf(ws).includes('runway'), 'not resurrected on the right by a recall');
  });
});

describe('placement persists with the layout', () => {
  test('round trip keeps placed', () => {
    let ws = run(start(), { type: 'activate', slot: 'centre', id: 'board' });
    ws = run(ws, { type: 'move', id: 'inspector', to: 'left' });
    const back = deserialiseWorkspace(SURFACES, serialiseWorkspace(ws));
    assert.deepEqual(back?.placed, ws.placed);
    assert.deepEqual(ws.placed, { inspector: 'left' });
  });

  test('a stored workspace without placed loads with {}', () => {
    const raw = JSON.stringify({ version: 1, layout: start().layout, views: {} });
    assert.deepEqual(deserialiseWorkspace(SURFACES, raw)?.placed, {});
  });

  test('a placed entry naming an unknown surface, a bad slot, or the centre is dropped', () => {
    const raw = JSON.stringify({
      version: 1,
      layout: start().layout,
      views: {},
      placed: { inspector: 'left', ghost: 'right', logs: 'centre', board: 'nonsense' },
    });
    const back = deserialiseWorkspace(SURFACES, raw);
    assert.deepEqual(back?.placed, { inspector: 'left' });
  });
});
