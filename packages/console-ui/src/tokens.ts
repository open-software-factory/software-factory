/**
 * Theme tokens.
 *
 * Both themes are derived from one blue-green base hue (210) with a teal
 * accent (170), so they read as the same product rather than two skins. The
 * light theme is not an inversion: an inverted dark palette produces grey mud
 * and glaring surfaces. The rules actually applied:
 *
 *  - Surfaces get *lighter* as they come forward in dark, *darker* in light.
 *    Elevation is a tone step in both, just in opposite directions.
 *  - Text lightness is chosen for contrast against its own background, not
 *    mirrored. Light-theme body text sits near L 0.30, not at 1 − 0.885.
 *  - Chroma rises slightly in light. The same chroma that reads as a tint on
 *    a dark surface reads as washed-out on a bright one.
 *  - State colours drop in lightness and gain chroma in light, or green and
 *    amber lose all separation against white.
 *
 * Contrast targets: body text ≥ 7:1 against its surface, muted text ≥ 4.5:1,
 * faint text ≥ 3:1 (labels and captions only, never body copy).
 */

export type ThemeName = 'dark' | 'light';

export type TokenName =
  | 'bg' | 'bg-canvas'
  | 'surface-1' | 'surface-2' | 'surface-3'
  | 'border-subtle' | 'border-strong'
  | 'fg' | 'fg-strong' | 'fg-muted' | 'fg-faint'
  | 'accent' | 'accent-quiet'
  | 'state-ok' | 'state-active' | 'state-blocked' | 'state-fail' | 'state-idle'
  | 'shadow-ambient' | 'shadow-key'
  | 'pane-sheen' | 'scrim'
  // The centre canvas gets its depth from light, not texture: a source at the
  // top and a vignette at the corners, both a step either side of the base.
  | 'canvas-base' | 'canvas-lit' | 'canvas-vignette'
  // Prototype scaffolding: the lab bar and the letterbox behind the simulated
  // viewport. Not product chrome, but they still have to follow the theme or
  // the prototype looks broken in one of them.
  | 'lab-bg' | 'stage-backdrop';

export type Palette = Record<TokenName, string>;

export const DARK: Palette = {
  'bg': 'oklch(0.165 0.014 210)',
  'bg-canvas': 'oklch(0.145 0.012 210)',
  'surface-1': 'oklch(0.200 0.015 210)',
  'surface-2': 'oklch(0.240 0.016 210)',
  'surface-3': 'oklch(0.280 0.017 210)',
  'border-subtle': 'oklch(0.243 0.010 210)',
  'border-strong': 'oklch(0.330 0.012 210)',
  'fg': 'oklch(0.885 0.010 210)',
  'fg-strong': 'oklch(0.920 0.008 210)',
  'fg-muted': 'oklch(0.710 0.012 210)',
  'fg-faint': 'oklch(0.550 0.012 210)',
  'accent': 'oklch(0.780 0.100 170)',
  'accent-quiet': 'oklch(0.420 0.055 170)',
  'state-ok': 'oklch(0.760 0.090 150)',
  'state-active': 'oklch(0.760 0.080 215)',
  'state-blocked': 'oklch(0.780 0.100 75)',
  'state-fail': 'oklch(0.720 0.150 25)',
  'state-idle': 'oklch(0.620 0.010 210)',
  'shadow-ambient': 'oklch(0 0 0 / 0.55)',
  'shadow-key': 'oklch(0 0 0 / 0.42)',
  // A raised surface catches light along its top edge.
  'pane-sheen': 'inset 0 1px 0 oklch(1 0 0 / 0.035)',
  'scrim': 'oklch(0 0 0 / 0.50)',
  'canvas-base': 'oklch(0.178 0.013 210)',
  'canvas-lit': 'oklch(0.225 0.016 210)',
  'canvas-vignette': 'oklch(0.135 0.012 212)',
  'lab-bg': 'oklch(0.125 0.010 210)',
  'stage-backdrop': 'oklch(0.115 0.010 210)',
};

export const LIGHT: Palette = {
  // The page is the deepest plane in dark; in light it is the *quietest*, so
  // the canvas is a shade below the chrome rather than above it.
  'bg': 'oklch(0.975 0.004 210)',
  'bg-canvas': 'oklch(0.955 0.006 210)',
  // Elevation steps toward white, and stops short of it — pure white surfaces
  // beside a white page lose the layering entirely.
  'surface-1': 'oklch(0.995 0.002 210)',
  'surface-2': 'oklch(0.965 0.006 210)',
  'surface-3': 'oklch(0.935 0.008 210)',
  'border-subtle': 'oklch(0.905 0.008 210)',
  'border-strong': 'oklch(0.820 0.012 210)',
  'fg': 'oklch(0.300 0.014 210)',
  'fg-strong': 'oklch(0.215 0.016 210)',
  'fg-muted': 'oklch(0.470 0.014 210)',
  'fg-faint': 'oklch(0.585 0.012 210)',
  // Accent darkens and gains chroma: the dark-theme accent is invisible on white.
  'accent': 'oklch(0.560 0.115 170)',
  'accent-quiet': 'oklch(0.900 0.040 170)',
  'state-ok': 'oklch(0.545 0.120 150)',
  'state-active': 'oklch(0.545 0.110 215)',
  'state-blocked': 'oklch(0.620 0.130 75)',
  'state-fail': 'oklch(0.540 0.180 25)',
  'state-idle': 'oklch(0.640 0.010 210)',
  // Shadows in light are a tinted grey, never black — black on white reads dirty.
  'shadow-ambient': 'oklch(0.55 0.02 210 / 0.20)',
  'shadow-key': 'oklch(0.55 0.02 210 / 0.13)',
  // A white sheen is invisible on a light surface, so definition comes from a
  // hairline ring instead. Same token, opposite mechanism.
  'pane-sheen': 'inset 0 0 0 1px oklch(0.55 0.02 210 / 0.07)',
  'scrim': 'oklch(0.30 0.02 210 / 0.28)',
  // In light the canvas sits *below* the chrome, and the light source is a
  // brightening rather than a lift — same structure, inverted direction.
  'canvas-base': 'oklch(0.940 0.006 210)',
  'canvas-lit': 'oklch(0.975 0.004 210)',
  'canvas-vignette': 'oklch(0.895 0.010 212)',
  'lab-bg': 'oklch(0.915 0.008 210)',
  'stage-backdrop': 'oklch(0.890 0.008 210)',
};

export const PALETTES: Record<ThemeName, Palette> = { dark: DARK, light: LIGHT };

/** Emit a palette as CSS custom properties for a `[data-theme]` scope. */
export function paletteToCss(theme: ThemeName): string {
  const palette = PALETTES[theme];
  const body = (Object.keys(palette) as TokenName[])
    .map(name => `  --${name}: ${palette[name]};`)
    .join('\n');
  return `[data-theme="${theme}"] {\n${body}\n}`;
}

export function themeStylesheet(): string {
  return (['dark', 'light'] as ThemeName[]).map(paletteToCss).join('\n\n');
}
