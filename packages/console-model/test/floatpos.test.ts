import { test, describe } from 'node:test';
import assert from 'node:assert/strict';

import { clampFloat, FLOAT_HOME, FLOAT_KEY, ORB_KEY, loadFloat, parseFloatPos, saveFloat } from '../src/floatpos.ts';
import { createStorage, memoryBackend } from '../src/storage.ts';

const port = async (seed?: Record<string, string>) => {
  const p = createStorage(memoryBackend(seed), [FLOAT_KEY]);
  await p.hydrate();
  return p;
};

describe('parsing is field-by-field', () => {
  test('nothing stored means home', () => {
    assert.deepEqual(parseFloatPos(null), FLOAT_HOME);
    assert.deepEqual(parseFloatPos('not json'), FLOAT_HOME);
  });

  test('a bad field falls back alone', () => {
    assert.deepEqual(parseFloatPos('{"dx":-120,"dy":"far"}'), { dx: -120, dy: 0 });
    assert.deepEqual(parseFloatPos('{"dx":null,"dy":40.6}'), { dx: 0, dy: 41 });
  });
});

describe('clamping keeps the panel on the stage', () => {
  const range = { minDx: -600, maxDx: 0, minDy: 0, maxDy: 300 };

  test('inside the range is untouched', () => {
    assert.deepEqual(clampFloat({ dx: -200, dy: 50 }, range), { dx: -200, dy: 50 });
  });

  test('outside the range snaps to the nearest edge', () => {
    assert.deepEqual(clampFloat({ dx: 80, dy: -20 }, range), { dx: 0, dy: 0 });
    assert.deepEqual(clampFloat({ dx: -9000, dy: 9000 }, range), { dx: -600, dy: 300 });
  });

  test('a range that cannot fit collapses to home rather than somewhere off-stage', () => {
    assert.deepEqual(clampFloat({ dx: -50, dy: 50 }, { minDx: 10, maxDx: -10, minDy: 5, maxDy: -5 }), FLOAT_HOME);
  });
});

describe('round trip through the storage port', () => {
  test('saved position is restored', async () => {
    const a = await port();
    saveFloat(a, { dx: -240, dy: 96 });
    const b = await port({ [FLOAT_KEY]: a.read(FLOAT_KEY)! });
    assert.deepEqual(loadFloat(b), { dx: -240, dy: 96 });
  });

  test('no port means home', () => {
    assert.deepEqual(loadFloat(null), FLOAT_HOME);
  });

  test('the orb keeps its own place under its own key', async () => {
    const p = createStorage(memoryBackend(), [FLOAT_KEY, ORB_KEY]);
    await p.hydrate();
    saveFloat(p, { dx: -100, dy: 20 });
    saveFloat(p, { dx: -400, dy: -300 }, ORB_KEY);
    assert.deepEqual(loadFloat(p), { dx: -100, dy: 20 });
    assert.deepEqual(loadFloat(p, ORB_KEY), { dx: -400, dy: -300 });
  });
});
