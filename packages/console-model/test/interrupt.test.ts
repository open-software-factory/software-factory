import { test, describe } from 'node:test';
import assert from 'node:assert/strict';

import { interruptReduce, NO_INTERRUPT, showsBanner } from '../src/interrupt.ts';
import type { InterruptAction, InterruptState } from '../src/interrupt.ts';

const run = (actions: InterruptAction[], from: InterruptState = NO_INTERRUPT) =>
  actions.reduce(interruptReduce, from);

describe('the pause is a fact, not a dialog', () => {
  test('raising opens the dialog and starts the audit log', () => {
    const s = run([{ type: 'raise' }]);
    assert.equal(s.status, 'open');
    assert.equal(s.log.length, 1);
  });

  test('minimise hides the dialog, keeps the pause, and shows the banner', () => {
    const s = run([{ type: 'raise' }, { type: 'minimize' }]);
    assert.equal(s.status, 'minimized');
    assert.ok(showsBanner(s));
    assert.equal(run([{ type: 'reopen' }], s).status, 'open');
  });

  test('raising again while minimised just brings the dialog back, without a second log entry', () => {
    const a = run([{ type: 'raise' }, { type: 'minimize' }]);
    const b = interruptReduce(a, { type: 'raise' });
    assert.equal(b.status, 'open');
    assert.equal(b.log.length, a.log.length);
  });
});

describe('operations leave the pause in force', () => {
  test('expand, playbook and abort each log once and never resolve', () => {
    const s = run([{ type: 'raise' }, { type: 'expand' }, { type: 'expand' }, { type: 'playbook' }, { type: 'abort' }, { type: 'abort' }]);
    assert.equal(s.status, 'open');
    assert.deepEqual([s.expanded, s.playbookRun, s.aborted], [true, true, true]);
    assert.equal(s.log.length, 4, 'raise + three operations, no duplicates');
  });
});

describe('resume needs a recorded justification', () => {
  test('resume with nothing written is refused and flags the field', () => {
    const s = run([{ type: 'raise' }, { type: 'resume' }]);
    assert.equal(s.status, 'open');
    assert.ok(s.needsJustification);
  });

  test('whitespace does not count', () => {
    const s = run([{ type: 'raise' }, { type: 'justify', text: '   ' }, { type: 'resume' }]);
    assert.equal(s.status, 'open');
    assert.ok(s.needsJustification);
  });

  test('typing clears the flag; resume then resolves and records the text', () => {
    const flagged = run([{ type: 'raise' }, { type: 'resume' }]);
    const typed = interruptReduce(flagged, { type: 'justify', text: 'Token revoked under PB-07' });
    assert.equal(typed.needsJustification, false);
    const done = interruptReduce(typed, { type: 'resume' });
    assert.equal(done.status, 'resolved');
    assert.ok(done.log.at(-1)?.includes('Token revoked under PB-07'));
    assert.equal(showsBanner(done), false);
  });

  test('a resolved interruption ignores further operations', () => {
    const done = run([{ type: 'raise' }, { type: 'justify', text: 'ok' }, { type: 'resume' }]);
    assert.equal(interruptReduce(done, { type: 'abort' }), done);
    assert.equal(interruptReduce(done, { type: 'minimize' }), done);
  });
});
