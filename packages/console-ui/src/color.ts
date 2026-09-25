/**
 * Just enough colour maths to check our own palettes.
 *
 * The point is not to build a colour library — it is that a contrast claim
 * should be a test, not a sentence in a design doc. Every token pair we rely
 * on is asserted against a WCAG ratio in packages/console-ui/test/contrast.test.ts.
 */

export interface Oklch {
  l: number;
  c: number;
  h: number;
  alpha: number;
}

/** Parses the `oklch(L C H)` / `oklch(L C H / A)` forms we actually write. */
export function parseOklch(value: string): Oklch {
  const match = value.trim().match(/^oklch\(\s*([\d.]+)\s+([\d.]+)\s+([\d.]+)\s*(?:\/\s*([\d.]+)\s*)?\)$/);
  if (!match) throw new Error(`not an oklch() colour: ${value}`);
  return {
    l: Number(match[1]),
    c: Number(match[2]),
    h: Number(match[3]),
    alpha: match[4] === undefined ? 1 : Number(match[4]),
  };
}

/** OKLCh → linear sRGB, clamped into gamut. */
export function toLinearRgb({ l, c, h }: Oklch): [number, number, number] {
  const rad = (h * Math.PI) / 180;
  const a = c * Math.cos(rad);
  const b = c * Math.sin(rad);

  const l_ = (l + 0.3963377774 * a + 0.2158037573 * b) ** 3;
  const m_ = (l - 0.1055613458 * a - 0.0638541728 * b) ** 3;
  const s_ = (l - 0.0894841775 * a - 1.291485548 * b) ** 3;

  const clamp = (v: number) => Math.min(1, Math.max(0, v));
  return [
    clamp(4.0767416621 * l_ - 3.3077115913 * m_ + 0.2309699292 * s_),
    clamp(-1.2684380046 * l_ + 2.6097574011 * m_ - 0.3413193965 * s_),
    clamp(-0.0041960863 * l_ - 0.7034186147 * m_ + 1.707614701 * s_),
  ];
}

/** WCAG relative luminance. Linear sRGB is already what the formula wants. */
export function luminance(colour: string): number {
  const [r, g, b] = toLinearRgb(parseOklch(colour));
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

/** WCAG 2.x contrast ratio, 1–21. */
export function contrast(a: string, b: string): number {
  const la = luminance(a);
  const lb = luminance(b);
  const [hi, lo] = la > lb ? [la, lb] : [lb, la];
  return (hi + 0.05) / (lo + 0.05);
}

export const AA_TEXT = 4.5;
export const AA_LARGE = 3;
export const AAA_TEXT = 7;
