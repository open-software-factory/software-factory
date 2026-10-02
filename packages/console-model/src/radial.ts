/**
 * Where a ring of items can go around a point that may sit at an edge.
 *
 * Angles are CSS degrees: 0 points right, 90 down, -90 (270) up. A full
 * circle is used when it fits; otherwise the longest arc that keeps every
 * item inside the box, and the radius grows so items on a short arc do not
 * overlap. Pure, so the orb's placement is tested without a browser.
 */
import type { Field } from './flick.ts';

/** `arc` is null for a full circle; otherwise the open sector the items sit in, in degrees clockwise. */
export interface RingLayout { angles: number[]; radius: number; arc: { start: number; span: number } | null }

export interface RingOptions {
  /** Base ring radius in px. */
  radius: number;
  /** Item diameter plus the gap wanted between neighbours, in px. */
  pitch: number;
  /** Clearance an item needs from the box edge, in px (half its size plus its label). */
  margin: number;
}

function allowedDegrees(pos: { x: number; y: number }, box: Field, radius: number, margin: number): boolean[] {
  return Array.from({ length: 360 }, (_, deg) => {
    const r = (deg * Math.PI) / 180;
    const x = pos.x + Math.cos(r) * radius, y = pos.y + Math.sin(r) * radius;
    return x >= box.left + margin && x <= box.right - margin && y >= box.top + margin && y <= box.bottom - margin;
  });
}

/** Longest run of allowed degrees, treating 359 → 0 as neighbours. */
function longestArc(allowed: boolean[]): { start: number; span: number } | null {
  const first = allowed.indexOf(false);
  if (first === -1) return { start: 0, span: 360 };
  let best = { start: 0, span: 0 }, run = 0, runStart = 0;
  for (let k = 1; k <= 360; k++) {
    const i = (first + k) % 360;
    if (!allowed[i]) { run = 0; continue; }
    if (run === 0) runStart = i;
    run++;
    if (run > best.span) best = { start: runStart, span: run };
  }
  return best.span ? best : null;
}

export function ringLayout(pos: { x: number; y: number }, box: Field, n: number, o: RingOptions): RingLayout {
  let radius = o.radius;
  // Growing the radius can shorten the arc, so settle it in a few passes.
  for (let pass = 0; pass < 4; pass++) {
    const arc = longestArc(allowedDegrees(pos, box, radius, o.margin));
    if (!arc) break;
    if (arc.span >= 360) {
      const need = (n * o.pitch) / (2 * Math.PI);
      if (need <= radius) return { angles: Array.from({ length: n }, (_, i) => -90 + (360 / n) * i), radius, arc: null };
      radius = Math.ceil(need);
      continue;
    }
    // Items sit inside the arc, with a half-pitch of clearance at each end.
    const spanRad = (arc.span * Math.PI) / 180;
    const need = n > 1 ? ((n - 1) * o.pitch) / spanRad : 0;
    if (need > radius) { radius = Math.ceil(need); continue; }
    const usable = arc.span - 1;
    const angles = n === 1
      ? [arc.start + usable / 2]
      : Array.from({ length: n }, (_, i) => arc.start + (usable * i) / (n - 1));
    return { angles: angles.map(a => ((a % 360) + 360) % 360), radius, arc: { start: arc.start, span: usable } };
  }
  // No room anywhere: a full circle at the last radius is still operable by keyboard.
  return { angles: Array.from({ length: n }, (_, i) => -90 + (360 / n) * i), radius, arc: null };
}
