/**
 * FloatingOrb: a circular control that floats above everything, can be
 * dragged anywhere, flicked (momentum, wall bounce), and pressed to open one
 * of three things — a radial menu, a regular menu, or a floating panel.
 *
 * Deliberately independent of the shell. It knows nothing about placement,
 * surfaces or intents; the host passes items and gets back an id. Menus are
 * React Aria menus for their keyboard and roles; the ring is CSS on top.
 * Physics lives in console-model/src/flick.ts so it is unit-tested in Node.
 */
import { createElement as h, useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';
import type { CSSProperties, ReactNode, RefObject } from 'react';
import { createPortal } from 'react-dom';
import { Menu, MenuItem } from 'react-aria-components/Menu';
import { Popover as RACPopover } from 'react-aria-components/Popover';
import { Overlay } from './overlay.ts';
import { FLICK, clampTo, releaseVelocity, speed, step } from '../../console-model/src/flick.ts';
import type { Body, Field, Sample } from '../../console-model/src/flick.ts';
import { ringLayout } from '../../console-model/src/radial.ts';

/** The host's UI scale: sizes in orb.css are rem, so the JS geometry follows the root font size. */
const rem = () => (typeof document === 'undefined' ? 16 : parseFloat(getComputedStyle(document.documentElement).fontSize) || 16);
/** Ring item diameter in px; matches .od-orb-item in orb.css (3.5rem). */
const item = () => 3.5 * rem();

export interface OrbItem {
  id: string;
  label: string;
  /** A short glyph for the ring; the label is read by assistive tech and shown on the regular menu. */
  glyph?: ReactNode;
  children?: readonly OrbItem[];
  /** Where the operator already is: marked, still selectable. */
  current?: boolean;
}

export type OrbMode = 'radial' | 'menu' | 'panel';
/** Floating: items hang in space. Plate: an opaque disc or sector covers what is underneath. */
export type OrbRing = 'floating' | 'plate';

export interface FloatingOrbProps {
  /** What a press opens. Radial is the default; the others stay as patterns the ring can reach. */
  mode?: OrbMode;
  items: readonly OrbItem[];
  onAction: (id: string) => void;
  /** The host draws the panel; the orb only opens, places and closes it. */
  renderPanel?: (close: () => void) => ReactNode;
  /** The panel can also be opened by the host (say, from a ring item) whatever the press opens. */
  panelOpen?: boolean;
  onPanelOpenChange?: (open: boolean) => void;
  ring?: OrbRing;
  /** The element the orb may roam in. Defaults to the viewport. */
  boundsRef?: RefObject<HTMLElement | null>;
  /** Where it starts, as an offset from its home in the bottom-right corner (dx, dy ≤ 0); default home. */
  offset?: { dx: number; dy: number };
  /** After a drop, a flick coming to rest or a keyboard nudge: the centre, and its offset from home. */
  onMove?: (pos: { x: number; y: number; dx: number; dy: number }) => void;
  label?: string;
  size?: number;
  /** Ring geometry for radial mode. */
  radius?: number;
  glyph?: ReactNode;
}

const INSET = 12;

function fieldFor(el: HTMLElement | null, size: number): Field {
  const w = el ? el.clientWidth : window.innerWidth;
  const hgt = el ? el.clientHeight : window.innerHeight;
  const r = size / 2 + INSET;
  return { left: r, top: r, right: Math.max(r, w - r), bottom: Math.max(r, hgt - r) };
}

/** How far a plate reaches past the item centres: half an item and a little air. */
const plateReach = () => item() / 2 + 14;
/** Degrees a plate sector runs past its end items, so at a wall it meets the edge and is clipped there. */
const PLATE_OVERRUN = 12;

/** SVG path for the plate: a disc, or a sector padded so the end items sit inside it. */
function platePath(radius: number, arc: { start: number; span: number } | null): { d: string; r: number } {
  const r = radius + plateReach();
  if (!arc) return { d: `M ${-r} 0 A ${r} ${r} 0 1 1 ${r} 0 A ${r} ${r} 0 1 1 ${-r} 0 Z`, r };
  const pad = (Math.asin(Math.min(1, (item() / 2 + 8) / radius)) * 180) / Math.PI + PLATE_OVERRUN;
  if (arc.span + 2 * pad >= 350) return { d: `M ${-r} 0 A ${r} ${r} 0 1 1 ${r} 0 A ${r} ${r} 0 1 1 ${-r} 0 Z`, r };
  const a0 = ((arc.start - pad) * Math.PI) / 180, a1 = ((arc.start + arc.span + pad) * Math.PI) / 180;
  const large = arc.span + 2 * pad > 180 ? 1 : 0;
  const p = (a: number) => `${(Math.cos(a) * r).toFixed(1)} ${(Math.sin(a) * r).toFixed(1)}`;
  return { d: `M 0 0 L ${p(a0)} A ${r} ${r} 0 ${large} 1 ${p(a1)} Z`, r };
}

const motionMs = (el: Element, token: string, fallback: number) =>
  (window as unknown as { __MOTION_MS?: number }).__MOTION_MS
    ?? (parseFloat(getComputedStyle(el).getPropertyValue(token)) || fallback);

/** A ring of menu items around a centre, with a second ring for a submenu. */
function RadialMenu({ items, radius: baseRadius, pos, box, ring, onAction, onClose, label }: {
  items: readonly OrbItem[]; radius: number; pos: { x: number; y: number }; box: Field; ring: OrbRing;
  onAction: (id: string) => void; onClose: () => void; label: string;
}) {
  const [path, setPath] = useState<OrbItem[]>([]);
  const level = path.length ? path[path.length - 1].children ?? [] : items;
  const shown: OrbItem[] = path.length ? [{ id: '__back', label: 'Back', glyph: '‹' }, ...level] : [...level];
  const ringRef = useRef<HTMLDivElement>(null);
  const plateRef = useRef<SVGSVGElement>(null);
  // Only the arc that fits: in a corner the ring opens towards the room.
  // Labels live inside the disc, so an item needs only its own half-size plus a little air.
  const disc = item();
  const { angles, radius, arc } = ringLayout(pos, box, shown.length, { radius: baseRadius, pitch: disc + 14, margin: disc / 2 + 4 });

  // Born in the orb: each item starts at the centre, small and faint, and travels out
  // along its spoke with a touch of overshoot. React Aria mounts menu items in a
  // second pass after the menu itself, so the animation starts from each item's ref
  // callback, once per element, rather than from a layout effect that finds none.
  const EASE = 'cubic-bezier(0.2, 0, 0.2, 1)';
  const STAGGER = 28;
  const born = useRef(new WeakSet<HTMLElement>());
  const birth = (el: HTMLElement | null, i: number) => {
    if (!el || born.current.has(el)) return;
    born.current.add(el);
    const dur = motionMs(el, '--dur-transform', 280);
    const a = el.style.getPropertyValue('--angle');
    const at = (d: number, s: number) => `rotate(${a}) translate(${(radius * d).toFixed(1)}px) rotate(calc(-1 * ${a})) scale(${s})`;
    el.animate([
      { transform: at(0, 0.2), opacity: 0, offset: 0 },
      { transform: at(0.6, 0.9), opacity: 1, offset: 0.55 },
      { transform: at(1.04, 1.02), opacity: 1, offset: 0.85 },
      { transform: at(1, 1), opacity: 1, offset: 1 },
    ], { duration: dur, delay: i * STAGGER, easing: EASE, fill: 'backwards' });
  };
  // The plate is ours, so it can grow from the centre in a layout effect, with the items.
  useLayoutEffect(() => {
    const el = plateRef.current;
    if (!el) return;
    const dur = motionMs(el, '--dur-transform', 280);
    el.animate([
      { transform: 'scale(0.1)', opacity: 0 },
      { transform: 'scale(1)', opacity: 1 },
    ], { duration: dur + (shown.length - 1) * STAGGER * 0.6, easing: EASE, fill: 'backwards' });
  }, [path, ring]);

  const act = (key: unknown) => {
    const id = String(key);
    if (id === '__back') { setPath(p => p.slice(0, -1)); return; }
    const item = shown.find(i => i.id === id);
    if (item?.children?.length) { setPath(p => [...p, item]); return; }
    onClose();
    onAction(id);
  };

  const plate = ring === 'plate' ? platePath(radius, arc) : null;
  const menu = h(Menu, {
    ref: ringRef,
    'aria-label': label,
    className: 'od-orb-ring',
    style: { '--radius': `${radius}px` } as CSSProperties,
    onAction: act,
    autoFocus: 'first',
    'data-od-id': 'orb-radial',
    'data-level': String(path.length),
    'data-ring': ring,
  }, ...shown.map((item, i) => {
    const angle = `${Math.round(angles[i] * 10) / 10}deg`;
    return h(MenuItem, {
      key: item.id, id: item.id, textValue: item.label,
      ref: (el: HTMLElement | null) => birth(el, i),
      className: 'od-orb-item',
      style: { '--angle': angle } as CSSProperties,
      'data-od-id': `orb-item-${item.id}`,
      'data-has-children': String(!!item.children?.length),
      'data-current': item.current ? 'true' : undefined,
      'aria-current': item.current ? 'page' : undefined,
    }, h('span', { className: 'od-orb-glyph', 'aria-hidden': 'true' }, item.glyph ?? item.label.charAt(0)),
       h('span', { className: 'od-orb-label' }, item.label));
  }));
  if (!plate) return menu;
  // The plate sits under the items and over the content; the orb itself stays on top of it.
  return h('div', { className: 'od-orb-plated' },
    h('svg', {
      ref: plateRef, className: 'od-orb-plate', 'aria-hidden': 'true', 'data-od-id': 'orb-plate',
      width: plate.r * 2, height: plate.r * 2, viewBox: `${-plate.r} ${-plate.r} ${plate.r * 2} ${plate.r * 2}`,
      style: { left: -plate.r, top: -plate.r } as CSSProperties,
    }, h('path', { d: plate.d })),
    menu);
}

/** A conventional menu beside the orb, one level with submenus flattened as headings. */
function ListMenu({ items, onAction, onClose, label }: {
  items: readonly OrbItem[]; onAction: (id: string) => void; onClose: () => void; label: string;
}) {
  const flat: Array<OrbItem & { depth: number }> = [];
  const walk = (list: readonly OrbItem[], depth: number) => list.forEach(i => { flat.push({ ...i, depth }); if (i.children) walk(i.children, depth + 1); });
  walk(items, 0);
  return h(Menu, {
    'aria-label': label, className: 'od-menu od-orb-list', autoFocus: 'first', 'data-od-id': 'orb-menu',
    onAction: (key: unknown) => { const item = flat.find(i => i.id === String(key)); if (item?.children?.length) return; onClose(); onAction(String(key)); },
  }, ...flat.map(item => h(MenuItem, {
    key: item.id, id: item.id, textValue: item.label, className: 'od-menu-item',
    style: { paddingLeft: `${10 + item.depth * 14}px` } as CSSProperties,
    'data-od-id': `orb-item-${item.id}`,
  }, h('span', { className: 'od-orb-glyph small', 'aria-hidden': 'true' }, item.glyph ?? item.label.charAt(0)),
     h('span', { className: 'od-menu-label' }, item.label))));
}

export function FloatingOrb({ mode = 'radial', items, onAction, renderPanel, panelOpen, onPanelOpenChange, ring = 'floating',
  boundsRef, offset, onMove, label = 'Assistant', size = 3.25 * rem(), radius = 5.25 * rem(), glyph = '◎' }: FloatingOrbProps) {
  const orbRef = useRef<HTMLButtonElement>(null);
  const [pos, setPos] = useState<{ x: number; y: number } | null>(null);
  const [open, setOpen] = useState(false);
  const [pressed, setPressed] = useState(false);
  const [dragging, setDragging] = useState(false);
  const raf = useRef(0);
  const body = useRef<Body>({ x: 0, y: 0, vx: 0, vy: 0 });

  const field = useCallback(() => fieldFor(boundsRef?.current ?? null, size), [boundsRef, size]);

  // Home is the bottom-right corner of the bounds; a remembered offset moves it from there.
  useLayoutEffect(() => {
    if (pos) return;
    const f = field();
    setPos(clampTo(f, f.right + (offset?.dx ?? 0), f.bottom + (offset?.dy ?? 0)));
  }, [pos, field, offset]);

  // Its place is an offset from the home corner, so when the bounds change size it
  // keeps that offset rather than its old coordinates: a corner orb stays in the corner.
  const offsetRef = useRef(offset ?? { dx: 0, dy: 0 });
  useEffect(() => {
    const el = boundsRef?.current;
    const onResize = () => setPos(p => {
      if (!p) return p;
      const f = field();
      return clampTo(f, f.right + offsetRef.current.dx, f.bottom + offsetRef.current.dy);
    });
    if (el && 'ResizeObserver' in window) { const ro = new ResizeObserver(onResize); ro.observe(el); return () => ro.disconnect(); }
    window.addEventListener('resize', onResize);
    return () => window.removeEventListener('resize', onResize);
  }, [boundsRef, field]);

  useEffect(() => () => cancelAnimationFrame(raf.current), []);

  // The orb gives birth to the ring: a small squeeze and release as it opens.
  useEffect(() => {
    const el = orbRef.current;
    if (!open || !el) return;
    const dur = motionMs(el, '--dur-panel', 200);
    el.animate([
      { transform: 'translate(-50%, -50%) scale(1)' },
      { transform: 'translate(-50%, -50%) scale(0.88)', offset: 0.35 },
      { transform: 'translate(-50%, -50%) scale(1)' },
    ], { duration: dur * 1.5, easing: 'cubic-bezier(0.2, 0, 0.2, 1)' });
  }, [open]);

  const report = (p: { x: number; y: number }) => {
    const f = field();
    offsetRef.current = { dx: Math.round(p.x - f.right), dy: Math.round(p.y - f.bottom) };
    onMove?.({ x: p.x, y: p.y, ...offsetRef.current });
  };
  const commit = (p: { x: number; y: number }) => { setPos(p); report(p); };

  const flick = (from: Body) => {
    cancelAnimationFrame(raf.current);
    body.current = from;
    let last = performance.now();
    const f = field();
    const tick = (now: number) => {
      const dt = Math.min(48, now - last); last = now;
      body.current = step(body.current, dt, f);
      setPos({ x: body.current.x, y: body.current.y });
      if (speed(body.current) > 0) raf.current = requestAnimationFrame(tick);
      else report(body.current);
    };
    raf.current = requestAnimationFrame(tick);
  };

  const onPointerDown = (e: React.PointerEvent<HTMLButtonElement>) => {
    if (e.button !== 0 || !pos) return;
    cancelAnimationFrame(raf.current);
    const el = e.currentTarget;
    // A synthetic event in a test has no live pointer to capture; the listeners below still work.
    try { el.setPointerCapture(e.pointerId); } catch { /* not a real pointer */ }
    const start = { x: e.clientX, y: e.clientY }, origin = { ...pos };
    const samples: Sample[] = [{ t: performance.now(), x: e.clientX, y: e.clientY }];
    let moved = false;
    setPressed(true);
    const move = (ev: PointerEvent) => {
      const dx = ev.clientX - start.x, dy = ev.clientY - start.y;
      if (!moved && Math.hypot(dx, dy) < 6) return;
      if (!moved) { moved = true; setDragging(true); setOpen(false); }
      samples.push({ t: performance.now(), x: ev.clientX, y: ev.clientY });
      if (samples.length > 12) samples.shift();
      setPos(clampTo(field(), origin.x + dx, origin.y + dy));
    };
    const up = (ev: PointerEvent) => {
      el.removeEventListener('pointermove', move); el.removeEventListener('pointerup', up); el.removeEventListener('pointercancel', up);
      try { el.releasePointerCapture(ev.pointerId); } catch { /* already released */ }
      setPressed(false); setDragging(false);
      if (!moved) { setOpen(o => !o); return; }
      // A pause before release is a drop, not a flick: the release itself is the last sample.
      samples.push({ t: performance.now(), x: ev.clientX, y: ev.clientY });
      const v = releaseVelocity(samples);
      const here = clampTo(field(), origin.x + (ev.clientX - start.x), origin.y + (ev.clientY - start.y));
      if (Math.hypot(v.vx, v.vy) >= FLICK.threshold) flick({ ...here, vx: v.vx, vy: v.vy });
      else commit(here);
    };
    el.addEventListener('pointermove', move); el.addEventListener('pointerup', up); el.addEventListener('pointercancel', up);
  };

  // Escape and outside pointer close the radial ring (the other modes have React Aria's).
  useEffect(() => {
    if (!open || mode !== 'radial') return;
    const key = (e: KeyboardEvent) => { if (e.key === 'Escape') { e.stopPropagation(); setOpen(false); orbRef.current?.focus(); } };
    const down = (e: PointerEvent) => {
      const t = e.target as Element | null;
      if (t?.closest('[data-od-id="orb"], [data-od-id="orb-radial"]')) return;
      setOpen(false);
    };
    window.addEventListener('keydown', key, true);
    window.addEventListener('pointerdown', down, true);
    return () => { window.removeEventListener('keydown', key, true); window.removeEventListener('pointerdown', down, true); };
  }, [open, mode]);

  if (!pos) return null;

  const portalTarget = boundsRef?.current ?? document.body;
  const menuLabel = `${label} menu`;

  const orb = h('button', {
    ref: orbRef,
    type: 'button',
    className: 'od-orb',
    'data-od-id': 'orb',
    'data-pressed': String(pressed),
    'data-dragging': String(dragging),
    'data-open': String(open),
    'aria-label': label,
    'aria-haspopup': mode === 'panel' ? 'dialog' : 'menu',
    'aria-expanded': open,
    style: { left: pos.x, top: pos.y, width: size, height: size } as CSSProperties,
    onPointerDown,
    onKeyDown: (e: React.KeyboardEvent) => {
      // Keyboard moves it too: arrows nudge, Enter/Space opens (native button).
      const d = { ArrowLeft: [-16, 0], ArrowRight: [16, 0], ArrowUp: [0, -16], ArrowDown: [0, 16] }[e.key as 'ArrowLeft'];
      if (!d) return;
      e.preventDefault();
      commit(clampTo(field(), pos.x + d[0], pos.y + d[1]));
    },
  }, h('span', { className: 'od-orb-face', 'aria-hidden': 'true' }, glyph));

  let surface: ReactNode = null;
  if (open && mode === 'radial') {
    const f = field();
    const box: Field = { left: f.left - size / 2, top: f.top - size / 2, right: f.right + size / 2, bottom: f.bottom + size / 2 };
    // On a ring every arrow walks it: right and down go clockwise, left and up back.
    // Handled here rather than by the menu so a re-layout mid-gesture cannot drop the step.
    const walk = (e: React.KeyboardEvent) => {
      const dir = e.key === 'ArrowRight' || e.key === 'ArrowDown' ? 1 : e.key === 'ArrowLeft' || e.key === 'ArrowUp' ? -1 : 0;
      if (!dir) return;
      const els = [...e.currentTarget.querySelectorAll<HTMLElement>('.od-orb-item')];
      const at = els.indexOf(document.activeElement as HTMLElement);
      if (at === -1) return;
      e.preventDefault(); e.stopPropagation();
      els[(at + dir + els.length) % els.length].focus();
    };
    surface = h('div', { className: 'od-orb-ringwrap', style: { left: pos.x, top: pos.y } as CSSProperties, onKeyDownCapture: walk },
      h(RadialMenu, { items, radius, pos, box, ring, label: menuLabel, onAction, onClose: () => { setOpen(false); orbRef.current?.focus(); } }));
  } else if (mode === 'menu') {
    // Which way the menu opens depends on where the orb sits in its field.
    const f = field();
    const placement = pos.y > (f.top + f.bottom) / 2 ? 'top' : 'bottom';
    surface = h(RACPopover, {
      triggerRef: orbRef, isOpen: open, onOpenChange: setOpen, placement, offset: 10,
      className: 'od-popover', UNSTABLE_portalContainer: portalTarget, 'data-od-id': 'orb-popover',
    }, h(ListMenu, { items, label: menuLabel, onAction, onClose: () => setOpen(false) }));
  }

  // The panel: opened by a press in panel mode, or by the host from anywhere (a ring item, say).
  // The same floating panel a docked panel floats into, parked beside the orb.
  let panel: ReactNode = null;
  const showPanel = mode === 'panel' ? open : !!panelOpen;
  const setPanel = (v: boolean) => { if (mode === 'panel') setOpen(v); else onPanelOpenChange?.(v); };
  if (renderPanel && (showPanel || mode === 'panel')) {
    const f = field();
    const w = 22.5 * rem(), hgt = 20 * rem(), gap = size / 2 + 10;
    const left = pos.x + gap + w <= f.right + size / 2 + INSET ? pos.x + gap : Math.max(INSET, pos.x - gap - w);
    const top = Math.max(INSET, Math.min(pos.y - hgt / 2, f.bottom + size / 2 + INSET - hgt - INSET));
    panel = h(Overlay, {
      id: 'orb-panel', label, isOpen: showPanel, onOpenChange: setPanel, modal: false, placement: 'right',
      className: 'od-orb-panel', portalContainer: portalTarget, triggerRef: orbRef, fallbackFocusRef: orbRef,
      panelStyle: { position: 'absolute', left, top, width: w } as CSSProperties,
    }, renderPanel(() => setPanel(false)));
  }

  return createPortal(h('div', { className: 'od-orb-layer', 'data-od-id': 'orb-layer' }, orb, surface, panel), portalTarget);
}
