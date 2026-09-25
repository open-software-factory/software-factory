import { test, describe } from 'node:test';
import assert from 'node:assert/strict';

import { noPopOut, popOutLabel } from '../src/popout.ts';
import type { PopOutPort } from '../src/popout.ts';

/** A host that can pop out: what the Tauri adapter must look like from the shell's side. */
function fakeHost(): PopOutPort & { windows: Map<string, string> } {
  const windows = new Map<string, string>();
  return {
    windows,
    available: true,
    async open(id, title) { const handle = `win-${windows.size + 1}`; windows.set(handle, `${id}: ${title}`); return handle; },
    async close(handle) { windows.delete(handle); },
  };
}

describe('pop-out is a port, not a call to window.open', () => {
  test('without a host the port refuses and the label says why', async () => {
    assert.equal(noPopOut.available, false);
    assert.equal(await noPopOut.open('runway', 'Runway'), null);
    assert.equal(popOutLabel(noPopOut, 'Runway'), 'Own window needs the desktop app');
  });

  test('with a host, open returns a handle and close takes it back', async () => {
    const host = fakeHost();
    const handle = await host.open('runway', 'Runway');
    assert.equal(handle, 'win-1');
    assert.equal(host.windows.get('win-1'), 'runway: Runway');
    await host.close(handle!);
    assert.equal(host.windows.size, 0);
    assert.equal(popOutLabel(host, 'Runway'), 'Open Runway in its own window');
  });

  test('the model never mentions window.open', async () => {
    const { readFileSync } = await import('node:fs');
    const src = readFileSync(new URL('../src/popout.ts', import.meta.url), 'utf8');
    assert.equal(/window\.open\(/.test(src), false);
  });
});
