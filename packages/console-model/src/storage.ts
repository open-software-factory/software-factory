/**
 * The storage port.
 *
 * Layout must be readable *synchronously* at first render, but the desktop
 * store (Tauri) is async. So the port loads once at boot and is synchronous
 * from then on: `hydrate()` awaits the backing store, `read()` returns the
 * snapshot, and `write()` is fire-and-forget. Async never reaches the reducer.
 *
 * No adapter for a real browser or desktop store lives in this file. Each
 * target supplies its own (see storage.browser.ts, and the Tauri adapter in
 * the app package). That is what lets the design build ship an artifact with
 * no Web Storage reference in it — the build fails if one appears, because
 * OpenDesign's preview then forces a sandboxed render.
 */

export interface StorageBackend {
  /** Called once at boot. May be async — a Tauri store is. */
  load(key: string): string | null | Promise<string | null>;
  /** Fire-and-forget. Failures are swallowed by the port, never thrown. */
  save(key: string, value: string): void | Promise<void>;
}

export interface StoragePort {
  hydrate(): Promise<void>;
  read(key: string): string | null;
  write(key: string, value: string): void;
  /** False when the backing store refused — callers may tell the user. */
  readonly available: boolean;
}

/** The default everywhere it matters: no host API, no failure modes. */
export function memoryBackend(seed?: Record<string, string>): StorageBackend {
  const data = new Map<string, string>(Object.entries(seed ?? {}));
  return {
    load: key => data.get(key) ?? null,
    save: (key, value) => void data.set(key, value),
  };
}

const KEYS = ['od-factory-shell'] as const;

/**
 * Wraps a backend so a hostile or missing store degrades to in-memory rather
 * than taking the app down. A sandboxed iframe throws on *read*, which once
 * killed the whole shell before it painted.
 */
export function createStorage(backend: StorageBackend, keys: readonly string[] = KEYS): StoragePort {
  const snapshot = new Map<string, string>();
  let available = true;

  return {
    get available() {
      return available;
    },
    async hydrate() {
      for (const key of keys) {
        try {
          const value = await backend.load(key);
          if (value !== null && value !== undefined) snapshot.set(key, value);
        } catch {
          available = false;
        }
      }
    },
    read(key) {
      return snapshot.get(key) ?? null;
    },
    write(key, value) {
      snapshot.set(key, value);
      try {
        const result = backend.save(key, value);
        if (result && typeof (result as Promise<void>).catch === 'function') {
          (result as Promise<void>).catch(() => {
            available = false;
          });
        }
      } catch {
        available = false; // quota, private mode, sandbox — never fatal
      }
    },
  };
}
