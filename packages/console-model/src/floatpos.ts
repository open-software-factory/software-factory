/**
 * Where the floating inspector was left. An offset from its anchored home
 * (the right edge, below the top bar), so it stays sensible when the window
 * changes size: dx ≤ 0 moves it left, dy ≥ 0 moves it down. Persisted through
 * the storage port under its own key; not layout, not a preference.
 */

import type { StoragePort } from './storage.ts';

export interface FloatPos { dx: number; dy: number }

export const FLOAT_KEY = 'od-factory-float';
/** The orb's offset from its own home, the bottom-right corner of the stage. */
export const ORB_KEY = 'od-factory-orb';
export const FLOAT_HOME: FloatPos = { dx: 0, dy: 0 };

export interface FloatRange { minDx: number; maxDx: number; minDy: number; maxDy: number }

const num = (v: unknown, fallback: number) => (typeof v === 'number' && Number.isFinite(v) ? v : fallback);

/** Unknown or corrupt fields fall back one at a time, never wholesale. */
export function parseFloatPos(raw: string | null): FloatPos {
  if (!raw) return FLOAT_HOME;
  let parsed: unknown;
  try { parsed = JSON.parse(raw); } catch { return FLOAT_HOME; }
  const v = parsed as Partial<FloatPos> | null;
  return { dx: Math.round(num(v?.dx, 0)), dy: Math.round(num(v?.dy, 0)) };
}

/** Keep the panel inside its bounds; a range that cannot fit collapses to home. */
export function clampFloat(pos: FloatPos, range: FloatRange): FloatPos {
  const dx = range.minDx > range.maxDx ? 0 : Math.min(range.maxDx, Math.max(range.minDx, pos.dx));
  const dy = range.minDy > range.maxDy ? 0 : Math.min(range.maxDy, Math.max(range.minDy, pos.dy));
  return { dx: Math.round(dx), dy: Math.round(dy) };
}

export function loadFloat(storage: StoragePort | null, key: string = FLOAT_KEY): FloatPos {
  return storage ? parseFloatPos(storage.read(key)) : FLOAT_HOME;
}

export function saveFloat(storage: StoragePort | null, pos: FloatPos, key: string = FLOAT_KEY): void {
  storage?.write(key, JSON.stringify(pos));
}
