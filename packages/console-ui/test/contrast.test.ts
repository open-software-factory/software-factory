import { test, describe } from 'node:test';
import assert from 'node:assert/strict';

import { AA_LARGE, AA_TEXT, AAA_TEXT, contrast, luminance } from '../src/color.ts';
import { PALETTES, paletteToCss, themeStylesheet } from '../src/tokens.ts';
import type { Palette, ThemeName, TokenName } from '../src/tokens.ts';

const THEMES: ThemeName[] = ['dark', 'light'];

/** Every surface a piece of text can sit on. */
const SURFACES: TokenName[] = ['bg', 'bg-canvas', 'surface-1', 'surface-2', 'surface-3'];

const ratio = (p: Palette, fg: TokenName, bg: TokenName) => contrast(p[fg], p[bg]);
const report = (t: ThemeName, fg: TokenName, bg: TokenName, r: number, min: number) =>
  `${t}: ${fg} on ${bg} = ${r.toFixed(2)}:1 (need ${min}:1)`;

describe('the colour maths itself', () => {
  test('black and white are the known extremes', () => {
    assert.ok(luminance('oklch(0 0 0)') < 0.001);
    assert.ok(luminance('oklch(1 0 0)') > 0.999);
    assert.ok(Math.abs(contrast('oklch(0 0 0)', 'oklch(1 0 0)') - 21) < 0.1);
  });

  test('contrast is symmetric', () => {
    const a = PALETTES.dark.fg;
    const b = PALETTES.dark['surface-1'];
    assert.equal(contrast(a, b).toFixed(6), contrast(b, a).toFixed(6));
  });

  test('a malformed colour is an error, not a silent zero', () => {
    assert.throws(() => luminance('#112233'));
  });
});

describe('body text is comfortably readable on every surface', () => {
  for (const theme of THEMES) {
    const p = PALETTES[theme];
    for (const surface of SURFACES) {
      test(`${theme}: fg on ${surface}`, () => {
        const r = ratio(p, 'fg', surface);
        assert.ok(r >= AAA_TEXT, report(theme, 'fg', surface, r, AAA_TEXT));
      });
    }
  }
});

describe('secondary text still clears AA', () => {
  for (const theme of THEMES) {
    const p = PALETTES[theme];
    for (const surface of ['surface-1', 'surface-2'] as TokenName[]) {
      test(`${theme}: fg-muted on ${surface}`, () => {
        const r = ratio(p, 'fg-muted', surface);
        assert.ok(r >= AA_TEXT, report(theme, 'fg-muted', surface, r, AA_TEXT));
      });
    }
  }
});

describe('faint text is for labels only, and clears the large-text floor', () => {
  for (const theme of THEMES) {
    test(`${theme}: fg-faint on surface-1`, () => {
      const r = ratio(PALETTES[theme], 'fg-faint', 'surface-1');
      assert.ok(r >= AA_LARGE, report(theme, 'fg-faint', 'surface-1', r, AA_LARGE));
    });
  }
});

describe('accent and state colours carry meaning, so they must be visible', () => {
  const meaningful: TokenName[] = ['accent', 'state-ok', 'state-active', 'state-blocked', 'state-fail'];
  for (const theme of THEMES) {
    for (const token of meaningful) {
      test(`${theme}: ${token} on surface-1`, () => {
        const r = ratio(PALETTES[theme], token, 'surface-1');
        assert.ok(r >= AA_LARGE, report(theme, token, 'surface-1', r, AA_LARGE));
      });
    }
  }
});

describe('the light theme is derived, not inverted', () => {
  test('elevation steps toward the viewer in both themes', () => {
    // dark: surfaces get lighter as they come forward. light: darker.
    const d = PALETTES.dark;
    assert.ok(luminance(d['surface-3']) > luminance(d['surface-1']));
    const l = PALETTES.light;
    assert.ok(luminance(l['surface-3']) < luminance(l['surface-1']));
  });

  test('light-theme text is not a mirrored lightness', () => {
    // An inverted palette would put body text near L 0.115 (1 − 0.885).
    const lightFg = Number(PALETTES.light.fg.match(/oklch\(([\d.]+)/)![1]);
    assert.ok(lightFg > 0.25, `light fg L=${lightFg} — an inversion would be far darker and muddier`);
  });

  test('no surface is pure white, or layering disappears against the page', () => {
    for (const token of SURFACES) {
      assert.ok(luminance(PALETTES.light[token]) < 0.995, `${token} is effectively pure white`);
    }
  });

  test('both themes define exactly the same tokens', () => {
    assert.deepEqual(Object.keys(PALETTES.dark).sort(), Object.keys(PALETTES.light).sort());
  });
});

describe('stylesheet emission', () => {
  test('a palette emits a scoped custom-property block', () => {
    const css = paletteToCss('light');
    assert.match(css, /^\[data-theme="light"\] \{/);
    assert.match(css, /--surface-1: oklch\(/);
  });

  test('the sheet carries both themes', () => {
    const css = themeStylesheet();
    assert.match(css, /\[data-theme="dark"\]/);
    assert.match(css, /\[data-theme="light"\]/);
  });
});
