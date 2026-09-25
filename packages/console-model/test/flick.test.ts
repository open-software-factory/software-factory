import { test, describe } from 'node:test';
import assert from 'node:assert/strict';

import { FLICK, releaseVelocity, settle, speed, step } from '../src/flick.ts';

const field = { left: 0, top: 0, right: 1000, bottom: 600 };

describe('release velocity', () => {
  test('comes from the last stretch of samples, not the whole drag', () => {
    const samples = [
      { t: 0, x: 0, y: 0 }, { t: 500, x: 10, y: 0 },      // slow start
      { t: 900, x: 100, y: 0 }, { t: 950, x: 200, y: 0 }, // fast finish
    ];
    const v = releaseVelocity(samples);
    assert.ok(v.vx > 1.5, `expected the fast finish, got ${v.vx}`);
    assert.equal(v.vy, 0);
  });

  test('one sample is no velocity', () => {
    assert.deepEqual(releaseVelocity([{ t: 0, x: 5, y: 5 }]), { vx: 0, vy: 0 });
  });
});

describe('a flick decelerates and stops', () => {
  test('speed falls every step and reaches zero', () => {
    let b = { x: 500, y: 300, vx: 1.2, vy: 0 };
    const s0 = speed(b);
    b = step(b, 16, field);
    assert.ok(speed(b) < s0);
    const done = settle({ x: 500, y: 300, vx: 1.2, vy: 0 }, field);
    assert.equal(speed(done.body), 0);
    assert.ok(done.ms < 3000, `settled in ${done.ms} ms`);
    assert.ok(done.body.x > 500, 'travelled in the flick direction');
  });

  test('a body at rest stays put', () => {
    assert.deepEqual(step({ x: 100, y: 100, vx: 0, vy: 0 }, 16, field), { x: 100, y: 100, vx: 0, vy: 0 });
  });
});

describe('walls reflect', () => {
  test('a wall reverses the normal component and keeps the tangential one', () => {
    // Heading right and down, 10 px from the right wall, fast.
    const b = { x: 990, y: 300, vx: 2, vy: 0.5 };
    const next = step(b, 16, field);
    assert.ok(next.vx < 0, 'x reversed');
    assert.ok(next.vy > 0, 'y kept its sign');
    assert.ok(Math.abs(next.vx) < 2 * Math.exp(-FLICK.friction * 16) + 1e-9, 'restitution took energy');
    assert.ok(next.x <= field.right && next.x >= field.left);
  });

  test('a flick towards a corner bounces and never leaves the field', () => {
    const done = settle({ x: 950, y: 550, vx: 3, vy: 3 }, field);
    assert.ok(done.bounces >= 2, `bounced ${done.bounces} times`);
    let b = { x: 950, y: 550, vx: 3, vy: 3 };
    for (let i = 0; i < 400; i++) {
      b = step(b, 16, field);
      assert.ok(b.x >= 0 && b.x <= 1000 && b.y >= 0 && b.y <= 600, `left the field at step ${i}`);
    }
  });

  test('the landing angle mirrors the incoming angle at a single wall', () => {
    const b = { x: 995, y: 100, vx: 1, vy: 1 };
    const next = step(b, 16, field);
    // Restitution only softens the normal component, so undo it before comparing angles.
    const inAngle = Math.atan2(b.vy, b.vx), outAngle = Math.atan2(next.vy, -next.vx / FLICK.restitution);
    assert.ok(Math.abs(inAngle - outAngle) < 1e-9, `reflection preserves the angle to the wall: ${inAngle} vs ${outAngle}`);
  });
});
