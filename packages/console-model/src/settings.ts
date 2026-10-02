/**
 * Settings are not layout.
 *
 * Preferences change how the shell looks, not what is placed where. Keeping
 * them out of `ShellState` stops the layout reducer absorbing anything that
 * happens to be a preference. They persist through the same storage port,
 * under their own key.
 *
 * There is no motion setting and no motion signal. Motion is on. The shell
 * does not read a media query, at boot or at call time, and nothing about the
 * host can turn animation off.
 */

import type { StoragePort } from './storage.ts';

export type Theme = 'dark' | 'light';

export interface Settings {
  theme: Theme;
}

export const SETTINGS_KEY = 'od-factory-settings';

export const DEFAULT_SETTINGS: Settings = { theme: 'dark' };

export interface Environment {
  prefersDark?: boolean;
}

export function seedSettings(env: Environment = {}): Settings {
  return { theme: env.prefersDark === false ? 'light' : 'dark' };
}

function isTheme(v: unknown): v is Theme {
  return v === 'dark' || v === 'light';
}

/** Unknown or corrupt fields fall back one at a time, never wholesale. */
export function parseSettings(raw: string | null, fallback: Settings): Settings {
  if (!raw) return fallback;
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return fallback;
  }
  const value = parsed as Partial<Settings> | null;
  return { theme: isTheme(value?.theme) ? value.theme : fallback.theme };
}

export function loadSettings(storage: StoragePort | null, env: Environment = {}): Settings {
  const seeded = seedSettings(env);
  if (!storage) return seeded;
  return parseSettings(storage.read(SETTINGS_KEY), seeded);
}

export function saveSettings(storage: StoragePort | null, settings: Settings): void {
  storage?.write(SETTINGS_KEY, JSON.stringify(settings));
}

export const nextTheme = (t: Theme): Theme => (t === 'dark' ? 'light' : 'dark');
