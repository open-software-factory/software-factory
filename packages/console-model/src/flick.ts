/**
 * Flick physics for a floating body (the orb), as data.
 *
 * Positions are the body's centre in px; velocities in px/ms. The field is
 * the box the centre may occupy — the host subtracts the body's radius and
 * any inset before calling. Momentum decays exponentially; a wall reverses
 * the normal component of velocity and keeps the tangential one, scaled by a
 * restitution, which is what "bounces off at the angle it hit" means.
 */

export interface Body { x: number; y: number; vx: number; vy: number }
export interface Field { left: number; top: number; right: number; bottom: number }

export interface FlickOptions {
  /** Decay per millisecond; 0.004 halves the speed roughly every 170 ms. */
  friction: number;
  /** Fraction of normal speed kept on a bounce. */
  restitution: number;
  /** Below this speed (px/ms) the body is considered at rest. */
  stop: number;
  /** Below this release speed no flick happens; the body just stays. */
  threshold: number;
}

export const FLICK: FlickOptions = { friction: 0.004, restitution: 0.55, stop: 0.02, threshold: 0.25 };

export interface Sample { t: number; x: number; y: number }

/** Release velocity from the last stretch of pointer samples (px/ms). */
export function releaseVelocity(samples: readonly Sample[], window = 90): { vx: number; vy: number } {
  if (samples.length < 2) return { vx: 0, vy: 0 };
  const last = samples[samples.length - 1];
  // Walk back while the sample is still inside the window; always keep at least two.
  let i = samples.length - 1;
  while (i > 0 && last.t - samples[i - 1].t <= window) i--;
  if (i === samples.length - 1) i--;
  const first = samples[i];
  const dt = Math.max(1, last.t - first.t);
  return { vx: (last.x - first.x) / dt, vy: (last.y - first.y) / dt };
}

export const speed = (b: { vx: number; vy: number }) => Math.hypot(b.vx, b.vy);

export function clampTo(field: Field, x: number, y: number): { x: number; y: number } {
  return {
    x: Math.min(field.right, Math.max(field.left, x)),
    y: Math.min(field.bottom, Math.max(field.top, y)),
  };
}

/** One integration step of `dt` milliseconds. Pure: returns the next body. */
export function step(b: Body, dt: number, field: Field, o: FlickOptions = FLICK): Body {
  const k = Math.exp(-o.friction * dt);
  let vx = b.vx * k, vy = b.vy * k;
  let x = b.x + vx * dt, y = b.y + vy * dt;
  // Reflect off each wall: reverse the normal component, keep the tangential one.
  if (x < field.left) { x = field.left + (field.left - x); vx = -vx * o.restitution; }
  else if (x > field.right) { x = field.right - (x - field.right); vx = -vx * o.restitution; }
  if (y < field.top) { y = field.top + (field.top - y); vy = -vy * o.restitution; }
  else if (y > field.bottom) { y = field.bottom - (y - field.bottom); vy = -vy * o.restitution; }
  ({ x, y } = clampTo(field, x, y));
  if (Math.hypot(vx, vy) < o.stop) { vx = 0; vy = 0; }
  return { x, y, vx, vy };
}

/** Where a flick ends, stepping at 16 ms. Bounded so a bad option set cannot spin forever. */
export function settle(b: Body, field: Field, o: FlickOptions = FLICK, maxMs = 8000): { body: Body; ms: number; bounces: number } {
  let body = b, ms = 0, bounces = 0;
  while (speed(body) > 0 && ms < maxMs) {
    const next = step(body, 16, field, o);
    if (Math.sign(next.vx) !== Math.sign(body.vx) && body.vx !== 0 && next.vx !== 0) bounces++;
    if (Math.sign(next.vy) !== Math.sign(body.vy) && body.vy !== 0 && next.vy !== 0) bounces++;
    body = next; ms += 16;
  }
  return { body, ms, bounces };
}
