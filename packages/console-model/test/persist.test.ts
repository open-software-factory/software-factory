import { test, describe } from 'node:test';
import assert from 'node:assert/strict';

import { fromPreset } from '../src/layout.ts';
import { SURFACES } from '../src/surfaces.ts';
import { deserialise, load, save, serialise, STORAGE_KEY } from '../src/persist.ts';
import { createStorage, memoryBackend } from '../src/storage.ts';
import { browserBackend } from '../src/storage.browser.ts';
import type { StorageBackend } from '../src/storage.ts';
import type { Registry } from '../src/types.ts';

const start = () =>
  fromPreset(SURFACES, {
    centre: { panels: ['floor', 'board'], active: 'board' },
    left: { panels: ['attention'], mode: 'rail', size: 300 },
  });

/** Reproduces the sandboxed-iframe trap: even reading throws. */
const hostile = (): StorageBackend => ({
  load() { throw new DOMException('The operation is insecure.', 'SecurityError'); },
  save() { throw new DOMException('The operation is insecure.', 'SecurityError'); },
});

/** Reproduces a Tauri-style store: everything is a promise. */
const asyncBackend = (): StorageBackend & { data: Map<string, string> } => {
  const data = new Map<string, string>();
  return {
    data,
    load: async key => data.get(key) ?? null,
    save: async (key, value) => void data.set(key, value),
  };
};

describe('the port keeps async out of the reducer', () => {
  test('an async backend is readable synchronously after hydrate', async () => {
    const backend = asyncBackend();
    backend.data.set(STORAGE_KEY, serialise(start()));
    const port = createStorage(backend);
    await port.hydrate();
    assert.deepEqual(load(port, SURFACES), start(), 'read() must not return a promise');
  });

  test('writes do not have to be awaited', async () => {
    const backend = asyncBackend();
    const port = createStorage(backend);
    await port.hydrate();
    save(port, start());
    assert.deepEqual(load(port, SURFACES), start(), 'the snapshot updates immediately');
    await new Promise(r => setTimeout(r, 0));
    assert.ok(backend.data.has(STORAGE_KEY), 'and the backend catches up');
  });

  test('reading before hydrate yields null rather than throwing', () => {
    assert.equal(load(createStorage(memoryBackend()), SURFACES), null);
  });
});

describe('hostile storage', () => {
  test('a backend that throws on read degrades instead of taking the app down', async () => {
    const port = createStorage(hostile());
    await port.hydrate();
    assert.equal(port.available, false);
    assert.equal(load(port, SURFACES), null);
  });

  test('a backend that throws on write reports unavailable, never throws', async () => {
    const port = createStorage(hostile());
    await port.hydrate();
    assert.doesNotThrow(() => save(port, start()));
    assert.equal(save(port, start()), false);
  });

  test('no storage at all is a supported configuration', () => {
    assert.equal(save(null, start()), false);
    assert.equal(load(null, SURFACES), null);
  });
});

describe('adapters', () => {
  test('memory is the default and needs no host API', async () => {
    const port = createStorage(memoryBackend());
    await port.hydrate();
    save(port, start());
    assert.deepEqual(load(port, SURFACES), start());
    assert.equal(port.available, true);
  });

  test('the browser adapter round-trips through a Storage-shaped object', async () => {
    const fake = new Map<string, string>();
    const port = createStorage(
      browserBackend({ getItem: k => fake.get(k) ?? null, setItem: (k, v) => void fake.set(k, v) } as Storage),
    );
    await port.hydrate();
    save(port, start());
    assert.ok(fake.has(STORAGE_KEY));
  });
});

describe('restoring is a merge, not a trust exercise', () => {
  test('corrupt JSON degrades to null', () => {
    assert.equal(deserialise(SURFACES, '{not json'), null);
  });

  test('a payload with no layout degrades to null', () => {
    assert.equal(deserialise(SURFACES, JSON.stringify({ version: 1 })), null);
  });

  test('a surface that no longer exists is dropped', () => {
    const raw = JSON.stringify({
      version: 1,
      layout: { ...start(), centre: { panels: ['floor', 'ghost'], active: 'ghost', mode: 'expanded', size: 0 } },
    });
    const restored = deserialise(SURFACES, raw)!;
    assert.deepEqual(restored.centre.panels, ['floor']);
    assert.equal(restored.centre.active, 'floor', 'current falls back to a surviving panel');
  });

  test('a placement the surface no longer accepts is dropped', () => {
    const registry: Registry = { ...SURFACES, inspector: { ...SURFACES.inspector, accepts: ['right'] } };
    const raw = JSON.stringify({
      version: 1,
      layout: { ...start(), bottom: { panels: ['inspector'], active: 'inspector', mode: 'expanded', size: 132 } },
    });
    assert.deepEqual(deserialise(registry, raw)!.bottom.panels, []);
  });

  test('a nonsense size falls back to the default', () => {
    const raw = JSON.stringify({
      version: 1,
      layout: { ...start(), left: { panels: ['attention'], active: 'attention', mode: 'rail', size: 'wide' } },
    });
    assert.equal(typeof deserialise(SURFACES, raw)!.left.size, 'number');
  });

  test('the payload is versioned', () => {
    assert.equal(JSON.parse(serialise(start())).version, 1);
  });
});
