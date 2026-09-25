/**
 * Shell vocabulary. Nothing here knows about React or the DOM — the whole
 * placement model is data, so it can be tested without a browser.
 */

export type Slot = 'centre' | 'left' | 'right' | 'bottom';

export const SLOTS: readonly Slot[] = ['centre', 'left', 'right', 'bottom'];

/**
 * A region's mode describes its *navigation*, not its contents.
 *
 *   hidden    the region is not shown at all
 *   rail      navigation as icons          (left only)
 *   nav       navigation at full width     (left only)
 *   expanded  no navigation of its own     (right, bottom, centre)
 *
 * Whether a panel is showing is decided by one thing and one thing only:
 * whether it is in `panels`. There is deliberately no mode that hides an open
 * panel — that produced two controls with identical results and a nav that
 * reported a panel as open while nothing was on screen.
 */
export type PaneMode = 'expanded' | 'rail' | 'nav' | 'hidden';

export type SurfaceId = string;

/**
 * What a surface declares about itself. Placement is negotiated against this,
 * never assumed by the shell.
 */
export interface SurfaceSpec {
  id: SurfaceId;
  label: string;
  glyph: string;
  /** Width appetite in px. The slot clamps the user's drag to this range. */
  min: number;
  max: number;
  /** Where it opens from closed, and which nav section it sits in while closed. */
  home: Slot;
  /**
   * Slots this surface will accept. Omitted means "anywhere" — deliberately
   * permissive until real views tell us which placements are nonsense.
   */
  accepts?: readonly Slot[];
}

export type Registry = Readonly<Record<SurfaceId, SurfaceSpec>>;

export interface PaneState {
  panels: SurfaceId[];
  active: SurfaceId | null;
  mode: PaneMode;
  /** Side panes use width; the bottom pane uses height. */
  size: number;
}

export type ShellState = Readonly<Record<Slot, PaneState>>;

export type Action =
  | { type: 'open'; id: SurfaceId; to?: Slot }
  | { type: 'activate'; slot: Slot; id: SurfaceId }
  | { type: 'closeSurface'; id: SurfaceId }
  | { type: 'closePane'; slot: Slot }
  | { type: 'toggleSurface'; id: SurfaceId }
  | { type: 'move'; id: SurfaceId; to: Slot }
  | { type: 'setMode'; slot: Slot; mode: PaneMode }
  | { type: 'setNavWide'; wide: boolean }
  | { type: 'toggleRegion'; slot: Slot }
  | { type: 'resize'; slot: Slot; size: number }
  /** Relative, for keyboard steps: never depends on what the caller last saw. */
  | { type: 'resizeBy'; slot: Slot; delta: number; min?: number; max?: number }
  | { type: 'restore'; state: ShellState };

/** Nav sections are derived, never declared — see nav.ts. */
export type NavSection = 'primary' | 'panel';
