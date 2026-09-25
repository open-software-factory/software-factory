/**
 * One jsdom for every DOM test in this package. WAAPI is replaced by a clock
 * the test drives, so lifecycle is proven without any rendered interpolation.
 */
import { JSDOM } from 'jsdom';

// React's production build has no act(); a host that exports NODE_ENV=production
// must not silently turn every DOM test into a failure.
if (process.env.NODE_ENV === 'production') process.env.NODE_ENV = 'test';

export const dom = new JSDOM('<!doctype html><html><body></body></html>', {
  url: 'http://localhost/', pretendToBeVisual: true,
});
export const win = dom.window;
// Every DOM constructor React Aria may instanceof-check, not a hand-picked few:
// a missing one (HTMLSelectElement, once) throws inside its event handlers.
const DOM_GLOBAL = /^(HTML\w*Element|SVG\w*Element|Node|Element|Text|Comment|DocumentFragment|Range|Selection|DOMRect|DOMRectReadOnly|NodeFilter|MutationObserver|Event|UIEvent|CustomEvent|FocusEvent|KeyboardEvent|MouseEvent|PointerEvent|InputEvent|CompositionEvent|WheelEvent|TouchEvent|DragEvent|ClipboardEvent|Image|CSSStyleDeclaration)$/;
for (const key of ['window', 'document', 'navigator', ...Object.getOwnPropertyNames(win).filter(k => DOM_GLOBAL.test(k))]) {
  Object.defineProperty(globalThis, key, { configurable: true, value: key === 'window' ? win : (win as unknown as Record<string, unknown>)[key] });
}
globalThis.getComputedStyle = win.getComputedStyle.bind(win);
globalThis.requestAnimationFrame = win.requestAnimationFrame.bind(win);
globalThis.cancelAnimationFrame = win.cancelAnimationFrame.bind(win);
(globalThis as Record<string, unknown>).IS_REACT_ACT_ENVIRONMENT = true;
win.matchMedia = () => ({ matches: false, addListener() {}, removeListener() {}, addEventListener() {}, removeEventListener() {} }) as unknown as MediaQueryList;
globalThis.ResizeObserver = class { observe() {} unobserve() {} disconnect() {} } as unknown as typeof ResizeObserver;
win.HTMLElement.prototype.scrollIntoView = function () {};
// jsdom has no CSS.escape; React Aria uses it to resolve the virtually focused row by id.
const cssEscape = (s: string) => String(s).replace(/[^a-zA-Z0-9_ -￿-]/g, c => '\\' + c);
(globalThis as Record<string, unknown>).CSS = { escape: cssEscape, supports: () => false };
(win as unknown as Record<string, unknown>).CSS = (globalThis as Record<string, unknown>).CSS;

export interface FakeAnimation { target: Element; frames: Keyframe[]; cancelled: boolean; finish: () => void }
export const animations: FakeAnimation[] = [];
win.Element.prototype.animate = function (frames: Keyframe[]) {
  let resolve!: () => void;
  let reject!: (reason: Error) => void;
  const finished = new Promise<void>((yes, no) => { resolve = yes; reject = no; });
  const record: FakeAnimation = { target: this, frames, cancelled: false, finish: resolve };
  animations.push(record);
  return {
    finished,
    cancel() { record.cancelled = true; reject(new Error('cancelled')); },
  } as unknown as Animation;
};
