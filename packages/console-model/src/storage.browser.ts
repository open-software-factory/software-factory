/**
 * Browser adapter. Kept in its own module on purpose.
 *
 * The design build never imports this file, so the design artifact contains no
 * reference to the Web Storage API. That matters because OpenDesign's preview
 * scans an artifact's source and forces a sandboxed inline render when it
 * finds one — which breaks relative imports and fetches.
 *
 * The desktop adapter is the mirror of this file and lives in the app package,
 * wrapping @tauri-apps/plugin-store. Layout belongs in the OS app-data
 * location on desktop, not in a webview origin.
 */

import type { StorageBackend } from './storage.ts';

export function browserBackend(source?: Storage): StorageBackend {
  const store = source ?? globalThis.localStorage;
  return {
    load: key => store.getItem(key),
    save: (key, value) => store.setItem(key, value),
  };
}
