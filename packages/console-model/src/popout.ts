/**
 * Pop-out (DESIGN.md §5.2.4): a surface can leave the shell for its own
 * window. The shell never calls `window.open` — on the desktop target the
 * host is Tauri and the window is a `WebviewWindow`; in a browser or the
 * design artifact there is no compliant host, so the port says so and the
 * control stays visible but disabled, in the same spirit as a refused slot.
 *
 * Only the contract lives here. The Tauri adapter lives in the app package.
 */

import type { SurfaceId } from './types.ts';

export interface PopOutPort {
  /** False when no compliant host exists; the control is shown disabled. */
  readonly available: boolean;
  /** Open the surface in its own window. Resolves to a handle id, or null if refused. */
  open(id: SurfaceId, title: string): Promise<string | null>;
  /** Bring a popped-out surface back into the shell. */
  close(handle: string): Promise<void>;
}

/** The design artifact and plain browsers: nothing to pop into, honestly. */
export const noPopOut: PopOutPort = {
  available: false,
  async open() { return null; },
  async close() { /* nothing was opened */ },
};

/**
 * What the shell asks a host for. A Tauri adapter implements this with
 * `WebviewWindow`; tests implement it with a map. The shell depends only on
 * the two verbs and the flag.
 */
export function popOutLabel(port: PopOutPort, surfaceLabel: string): string {
  return port.available ? `Open ${surfaceLabel} in its own window` : `Own window needs the desktop app`;
}
