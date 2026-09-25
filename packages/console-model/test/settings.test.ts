import { test, describe } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';

import {
  DEFAULT_SETTINGS, loadSettings, nextTheme, parseSettings, saveSettings, seedSettings, SETTINGS_KEY,
} from '../src/settings.ts';
import { createStorage, memoryBackend } from '../src/storage.ts';

const port = async (seed?: Record<string, string>) => {
  const p = createStorage(memoryBackend(seed), [SETTINGS_KEY]);
  await p.hydrate();
  return p;
};

describe('there is no motion setting and no motion signal', () => {
  test('settings carry theme and nothing else', () => {
    assert.deepEqual(Object.keys(DEFAULT_SETTINGS), ['theme']);
  });

  test('seeding takes nothing from the host but the colour scheme', () => {
    assert.deepEqual(seedSettings({ prefersDark: false }), { theme: 'light' });
    assert.deepEqual(seedSettings(), { theme: 'dark' });
  });

  /*
    A regression guard with teeth. Reading a motion preference from the host
    silently disabled animation three separate times — twice shipped. The rule
    is now structural: the source may not mention it at all.
  */
  test('no source file mentions a motion media query', () => {
    const dir = join(import.meta.dirname, '..', 'src');
    const offenders: string[] = [];
    for (const file of readdirSync(dir)) {
      const text = readFileSync(join(dir, file), 'utf8');
      if (/prefers-reduced-motion|prefersReducedMotion/.test(text)) offenders.push(file);
    }
    assert.deepEqual(offenders, [], 'motion must never be gated on a host signal');
  });
});

describe('parsing is field-by-field', () => {
  test('corrupt JSON falls back whole', () => {
    assert.deepEqual(parseSettings('{nope', DEFAULT_SETTINGS), DEFAULT_SETTINGS);
  });

  test('an unknown theme falls back', () => {
    assert.equal(parseSettings(JSON.stringify({ theme: 'solarized' }), DEFAULT_SETTINGS).theme, 'dark');
  });

  test('a stored choice beats the seed', async () => {
    const storage = await port({ [SETTINGS_KEY]: JSON.stringify({ theme: 'light' }) });
    assert.equal(loadSettings(storage, { prefersDark: true }).theme, 'light');
  });
});

describe('round trip', () => {
  test('settings survive save and load', async () => {
    const storage = await port();
    saveSettings(storage, { theme: 'light' });
    assert.deepEqual(loadSettings(storage), { theme: 'light' });
  });

  test('settings use their own key, so layout and preferences cannot clobber each other', async () => {
    const storage = await port();
    saveSettings(storage, { theme: 'light' });
    assert.ok(storage.read(SETTINGS_KEY));
    assert.equal(storage.read('od-factory-shell'), null);
  });

  test('no storage is a supported configuration', () => {
    assert.deepEqual(loadSettings(null), DEFAULT_SETTINGS);
    assert.doesNotThrow(() => saveSettings(null, DEFAULT_SETTINGS));
  });
});

describe('helpers', () => {
  test('theme toggles both ways', () => {
    assert.equal(nextTheme('dark'), 'light');
    assert.equal(nextTheme('light'), 'dark');
  });
});
