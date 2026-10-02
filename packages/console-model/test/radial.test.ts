import { test, describe } from 'node:test';
import assert from 'node:assert/strict';

import { ringLayout } from '../src/radial.ts';

const box = { left: 0, top: 0, right: 1000, bottom: 600 };
const opts = { radius: 84, pitch: 64, margin: 42 };

const at = (pos: { x: number; y: number }, angle: number, radius: number) => ({
  x: pos.x + Math.cos((angle * Math.PI) / 180) * radius,
  y: pos.y + Math.sin((angle * Math.PI) / 180) * radius,
});

const inside = (p: { x: number; y: number }) =>
  p.x >= box.left + opts.margin && p.x <= box.right - opts.margin && p.y >= box.top + opts.margin && p.y <= box.bottom - opts.margin;

describe('ring layout', () => {
  test('in the open, a full circle starting at the top', () => {
    const { angles, radius, arc } = ringLayout({ x: 500, y: 300 }, box, 5, opts);
    assert.equal(radius, 84);
    assert.deepEqual(angles.map(Math.round), [-90, -18, 54, 126, 198]);
    assert.equal(arc, null);
  });

  test('the reported arc contains every item angle', () => {
    const { angles, arc } = ringLayout({ x: 962, y: 562 }, box, 5, opts);
    assert.ok(arc, 'a corner is not a full circle');
    for (const a of angles) {
      const rel = ((a - arc!.start) % 360 + 360) % 360;
      assert.ok(rel <= arc!.span + 1e-9, `angle ${Math.round(a)} outside arc ${arc!.start}+${arc!.span}`);
    }
  });

  test('in a corner, every item stays inside the box', () => {
    const pos = { x: 962, y: 562 };
    const { angles, radius } = ringLayout(pos, box, 5, opts);
    for (const a of angles) assert.ok(inside(at(pos, a, radius)), `angle ${Math.round(a)} at radius ${radius} left the box`);
    // Only the quadrant towards the centre is open: between pointing left (180) and up (270).
    for (const a of angles) assert.ok(a >= 170 && a <= 280, `angle ${Math.round(a)} outside the open quadrant`);
  });

  test('a short arc grows the radius so items do not overlap', () => {
    const pos = { x: 962, y: 562 };
    const { angles, radius } = ringLayout(pos, box, 5, opts);
    assert.ok(radius > 84, `radius ${radius}`);
    for (let i = 1; i < angles.length; i++) {
      const a = at(pos, angles[i - 1], radius), b = at(pos, angles[i], radius);
      assert.ok(Math.hypot(a.x - b.x, a.y - b.y) >= opts.pitch - 1, `items ${i - 1} and ${i} overlap`);
    }
  });

  test('at one edge, a half circle facing inward', () => {
    const pos = { x: 500, y: 570 };
    const { angles, radius } = ringLayout(pos, box, 3, opts);
    for (const a of angles) assert.ok(inside(at(pos, a, radius)));
    for (const a of angles) assert.ok(a >= 180 && a <= 360, `angle ${Math.round(a)} points down into the edge`);
  });

  test('one item sits in the middle of the arc', () => {
    const { angles } = ringLayout({ x: 500, y: 570 }, box, 1, opts);
    assert.equal(angles.length, 1);
    assert.ok(Math.abs(angles[0] - 270) < 2, `angle ${angles[0]}`);
  });
});
