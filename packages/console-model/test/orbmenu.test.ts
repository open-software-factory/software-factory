import { test, describe } from 'node:test';
import assert from 'node:assert/strict';

import { orbIntentOf, orbMenu } from '../src/orbmenu.ts';
import { fromPreset } from '../src/layout.ts';
import { NAV_ORDER, SURFACES } from '../src/surfaces.ts';

const state = fromPreset(SURFACES, {
  centre: { panels: ['floor', 'board', 'runway'], active: 'board', mode: 'expanded', size: 0 },
  left: { panels: ['attention'], active: 'attention', mode: 'rail', size: 280 },
  right: { panels: [], active: null, mode: 'hidden', size: 320 },
  bottom: { panels: [], active: null, mode: 'expanded', size: 200 },
});

const ctx = (over: Partial<Parameters<typeof orbMenu>[0]> = {}) =>
  orbMenu({ registry: SURFACES, state, order: NAV_ORDER, navHidden: false, selection: null, ...over });

const labels = (list: ReturnType<typeof orbMenu>) => list.map(e => e.label);

describe('the ring with the nav showing', () => {
  test('assistant, commands and a Go to submenu; context is the current view only', () => {
    const ring = ctx();
    assert.deepEqual(labels(ring), ['Ask', 'Voice', 'Commands', 'Go to', 'Here']);
    const go = ring.find(e => e.id === 'go')!;
    assert.deepEqual(labels(go.children!), ['Floor', 'Board', 'Runway']);
    assert.equal(go.children!.find(e => e.id === 'view:board')!.current, true);
    assert.equal(go.children!.find(e => e.id === 'view:floor')!.current, false);
    const here = ring.find(e => e.id === 'here')!;
    assert.deepEqual(labels(here.children!), ['Ask about Board']);
  });

  test('a selection adds inspect and ask-about, and names the context after it', () => {
    const ring = ctx({ selection: { ref: 'service-auth#5', title: 'Keycloak' } });
    const here = ring.find(e => e.id === 'here')!;
    assert.equal(here.label, 'service-auth#5');
    assert.deepEqual(labels(here.children!), ['Inspect service-auth#5', 'Ask about service-auth#5', 'Ask about Board']);
    assert.deepEqual(orbIntentOf(ring, 'inspect:service-auth#5'), { kind: 'inspect', ref: 'service-auth#5' });
    assert.deepEqual(orbIntentOf(ring, 'about:service-auth#5'), { kind: 'assistant', mode: 'ask', about: 'service-auth#5' });
  });
});

describe('the ring as the navigation', () => {
  test('with the nav off screen the views sit on the ring and the panels get a submenu', () => {
    const ring = ctx({ navHidden: true });
    assert.deepEqual(labels(ring), ['Ask', 'Voice', 'Commands', 'Floor', 'Board', 'Runway', 'Panels', 'Here']);
    const panels = ring.find(e => e.id === 'panels')!;
    assert.deepEqual(labels(panels.children!), ['Close Attention', 'Open Flight recorder', 'Open Run output']);
    assert.deepEqual(orbIntentOf(ring, 'panel:logs'), { kind: 'toggle', id: 'logs' });
  });

  test('the ring never offers to hide or show the nav', () => {
    const all = (list: ReturnType<typeof orbMenu>): string[] => list.flatMap(e => [e.id, ...(e.children ? all(e.children) : [])]);
    for (const navHidden of [false, true]) assert.ok(all(ctx({ navHidden })).every(id => !id.startsWith('nav:')));
  });

  test('view entries carry the same intent the palette uses', () => {
    assert.deepEqual(orbIntentOf(ctx(), 'view:runway'), { kind: 'view', id: 'runway' });
    assert.deepEqual(orbIntentOf(ctx(), 'commands'), { kind: 'palette' });
    assert.equal(orbIntentOf(ctx(), 'go'), undefined);
    assert.equal(orbIntentOf(ctx(), 'nope'), undefined);
  });
});
