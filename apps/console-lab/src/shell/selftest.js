/* ── SELF-TEST ───────────────────────────────────────────────────────────────
   Drives the real controls and samples computed values across frames. Event
   counting proved unreliable; interpolation of the measured value is the truth.
   Set SELFTEST = true to re-run the harness on load.                          */
import React from 'react';
import { ORB_KEY } from '@open-software-factory/console-model';
import { store } from './App.jsx';

export const SELFTEST = false; /* flip to true to re-run the harness on load */
const SUITE = 'AC';     /* AC orb ring follows the nav; AD orb context + assistant; AE orb as the nav in focus mode; AF orb place remembered; AG panels submenu, Esc back, smallest size; AH inspect from the ring; AB polish; AA breadcrumb; Z drag-rearrange; Y per-view layout; W keyboard parity; T+U+V float; S popovers; Q+R P0 dialog; X measures gaps; I labels + toggles; L+M inspector; K palette; J drawer; P opens the palette. */
if (SELFTEST) setTimeout(() => {
  const lines = [];
  const box = document.createElement('pre'); box.id = 'selftest'; box.textContent = 'self-test…';
  document.body.appendChild(box);
  /* The exporter snapshots ~660ms after boot, so a suite has to finish inside
     that. Motion is shortened for the run and the suite is split in two.     */
  /* Long enough that a frame sampler sees the middle of a transition, short
     enough that a suite still fits the exporter's capture window. At 40ms an
     animation is over in two frames and reads as a snap to the sampler. */
  window.__MOTION_MS = 80;
  const SETTLE = 55;
  const t0 = performance.now();
  const paint = () => { box.textContent = 'suite ' + SUITE + ' · t=' + Math.round(performance.now() - t0) + 'ms\n' + lines.join('\n'); };
  setInterval(paint, 20);
  const frame = () => new Promise(r => requestAnimationFrame(r));
  const wait = (n) => new Promise(r => setTimeout(r, n));
  const q = (s) => document.querySelector(s);
  const width = (s) => { const e = q(s); return e ? Math.round(e.getBoundingClientRect().width) : -1; };
  /* the bottom bar is full-bleed, so only its height says whether it is open */
  const height = (s) => { const e = q(s); return e ? Math.round(e.getBoundingClientRect().height) : -1; };

  async function sampleDuring(sel, fn, n = 5) {
    const before = width(sel);
    fn();
    const seen = [];
    for (let i = 0; i < n; i++) { await frame(); seen.push(width(sel)); }
    return { before, seen, after: seen[seen.length - 1], moved: new Set(seen).size > 2 };
  }
  const row = (name, ok, detail) => { lines.push(name.padEnd(26) + (ok ? 'PASS  ' : 'FAIL  ') + detail); paint(); };

  /* Only the face that is actually operable counts. */
  const railCount = () => document.querySelectorAll('[data-side="left"] .railface:not([inert]) .railbtn').length;

  /*
    Wait for the layout to stop moving instead of guessing a delay.

    The stage is in the signature deliberately: the renderer resizes its own
    viewport during boot (1926 → 1443 here), so panes can be at rest while the
    space around them is still changing. Watching only the panes produced a
    failure that vanished on re-run — the worst kind of test.
  */
  async function settled(tries = 60) {
    let last = '';
    let stable = 0;
    for (let i = 0; i < tries; i++) {
      const now = ['left', 'centre', 'right'].map(s => width(`[data-side="${s}"]`)).join(',')
        + '|' + width('[data-od-id="app"]');
      stable = now === last ? stable + 1 : 0;
      if (stable >= 3) return now;
      last = now;
      await frame();
    }
    return last;
  }

  /*
    The preset switcher is gone, so setup goes through the same controls an
    operator uses. That is the point: if a suite cannot reach a state this way,
    neither can the user, and the state does not really exist.
  */
  const navSwitch = (label) =>
    Array.from(document.querySelectorAll('[data-side="left"] .railface:not([inert]) .nav button, [data-od-id="drawer"] .nav button'))
      .find(b => b.textContent.startsWith(label));

  const ui = {
    /* rail ↔ full-width navigation, via the toggle at the foot of the rail */
    async expandNav() { q('[data-od-id="nav-toggle"]').click(); await settled(); },
    /* open or close a panel by its nav label */
    async togglePanel(label) { navSwitch(label)?.click(); await settled(); },
    async focus() { q('[data-od-id="focus-toggle"]').click(); await settled(); },
    /* the tabbed-right state: inspector plus a second right-hand panel */
    async tabbedRight() { await ui.togglePanel('Run output'); },
  };

  async function suiteA() {
    lines.push('react ' + React.version + ' · motion ' + window.__MOTION_MS + 'ms'); paint();

    /* The first thing to prove is the thing that shipped broken twice: opening
       the artifact gives you motion, without touching a control. */
    row('animation on by default', q('[data-anim]').dataset.anim === 'true',
      'data-anim=' + q('[data-anim]').dataset.anim);

    // 1 — hiding the right pane must animate AND give the space to the centre
    const rest = await settled();
    lines.push('at rest L,C,R          ' + rest + ' · bp ' + q('#bpnote').textContent); paint();
    const c0 = width('[data-side="centre"]');
    let r = await sampleDuring('[data-side="right"]', () => q('[data-region="right"]').click());
    await wait(SETTLE);
    row('right hide animates', r.moved, r.seen.join(' '));
    row('centre absorbs space', width('[data-side="centre"]') > c0 + 100,
      `centre ${c0} → ${width('[data-side="centre"]')} · L ${width('[data-side="left"]')} R ${width('[data-side="right"]')}`);

    // 2 — and coming back
    r = await sampleDuring('[data-side="right"]', () => q('[data-region="right"]').click());
    row('right show animates', r.moved, r.seen.join(' '));

    // 3 — a navigation width change animates
    r = await sampleDuring('[data-side="left"]', () => q('[data-od-id="nav-toggle"]').click());
    row('nav width animates', r.moved, r.seen.join(' '));
    q('[data-od-id="nav-toggle"]').click(); await wait(SETTLE);

    /* 4 — one action per panel. A second control that produced the same
       picture is what made "collapse" and "close" indistinguishable. */
    /* One *close* per panel, and no second control that draws the same
       picture (the old "collapse"). Pop-out is a different outcome and is
       allowed beside it. */
    const paneActions = Array.from(document.querySelectorAll('[data-side="left"] .pane-actions button'));
    const acts = paneActions.map(b => b.dataset.act);
    row('one close per panel', acts.filter(a => a === 'close').length === 1 && !acts.includes('collapse'),
      acts.join(' · '));

    // 5 — tabs appear only when a pane holds more than one panel
    const oneTab = q('[data-side="right"] .tabs');
    await ui.tabbedRight();
    const twoTabs = q('[data-side="right"] .tabs');
    row('tabs only when >1', !oneTab && !!twoTabs, `1 panel:${oneTab ? 'tabs' : 'header'} · 2 panels:${twoTabs ? 'tabs' : 'header'}`);

    // 6 — centre tab switch animates the body
    let anims = 0;
    const native = Element.prototype.animate;
    Element.prototype.animate = function (...a) { anims++; return native.apply(this, a); };
    q('[data-side="centre"] .tab:nth-child(2)')?.click();
    await frame(); await frame();
    Element.prototype.animate = native;
    row('centre tab animates', anims > 0, anims + ' animation(s)');

  }

  /* Suite H — the transient bottom region and the narrow-viewport drawer.
     Split out of A: a suite has to finish inside the exporter's capture
     window, and A had grown past it. */
  async function suiteH() {
    lines.push('bottom region · narrow viewport'); paint();
    await settled();

    await sampleDuring('[data-od-id="bottom"]', () => q('[data-region="bottom"]').click(), 3);
    await settled();
    row('bottom opens', height('[data-od-id="bottom"]') > 40, 'height ' + height('[data-od-id="bottom"]'));
    q('[data-region="bottom"]').click(); await settled();
    row('and closes again', height('[data-od-id="bottom"]') < 4, 'height ' + height('[data-od-id="bottom"]'));

    q('[data-w="820"]').click(); await settled();
    const ham = q('[data-od-id="hamburger"]');
    row('narrow → hamburger', !!ham, ham ? 'shown' : 'missing');
    if (ham) {
      ham.click(); await wait(SETTLE);
      row('drawer opens', q('[data-od-id="drawer"]') !== null, 'on');
      row('drawer carries the nav', q('[data-od-id="drawer"]').querySelectorAll('.nav button').length > 0,
        'navigation reachable at narrow widths');
      q('[data-od-id="drawer-close"]').click();
    }
    q('[data-w="0"]').click();
  }

  async function suiteB() {
    lines.push('close/collapse rules · motion ' + window.__MOTION_MS + 'ms'); paint();

    // 1 — closing the last tab of a 2-panel pane collapses that pane
    await ui.tabbedRight();
    /* Close the first from its tab, the last from the header — because the tab
       strip only exists while more than one panel is open, which is the rule
       this shell deliberately has. Re-query between clicks: React reuses DOM
       nodes, so a list captured up front leaves you clicking a detached one. */
    q('[data-side="right"] .tab .x').click(); await wait(SETTLE);
    q('[data-side="right"] [data-act="close"]').click();
    await settled();
    row('empty pane collapses', width('[data-side="right"]') < 4,
      `right width ${width('[data-side="right"]')} · inspector ${window.__placement('inspector')} · logs ${window.__placement('logs')}`);

    // 2 — closing the left pane keeps the nav rail alive, and animates
    const before = railCount();
    const r = await sampleDuring('[data-side="left"]', () => q('[data-side="left"] [data-act="close"]').click());
    await wait(SETTLE);
    row('close left keeps nav', railCount() > 0, `rail items ${before} → ${railCount()} · width ${r.before} → ${width('[data-side="left"]')}`);
    row('close left animates', r.moved, r.seen.join(' '));

    /* 3 — the rail widens into full navigation. It must stay one navigation:
       no panel, no tab, and never the rail and the wide list at once. */
    /* Both faces are mounted so they can cross-fade, so "which navigation is
       showing" is a state to read, not an element to look for. */
    const stack = () => q('[data-side="left"] .railstack');
    const wRail = width('[data-side="left"]');
    await ui.expandNav();
    await settled();
    row('rail widens into nav', stack().dataset.wide === 'true' && width('[data-side="left"]') > wRail,
      `${wRail} → ${width('[data-side="left"]')}px`);
    row('it is still one navigation', !q('[data-side="left"] .tabs') && !q('[data-side="left"] .pane-head'),
      'no tab, no panel header');
    row('labels replace icons', (q('[data-od-id="nav-wide"]')?.querySelectorAll('.nav button').length ?? 0) === 6,
      'six labelled items');
    row('only one face is operable', document.querySelectorAll('[data-side="left"] .railface:not([inert])').length === 1,
      'the other is inert');

    await ui.expandNav();
    await settled();
    row('and collapses back', stack().dataset.wide === 'false' && railCount() > 0, `rail items ${railCount()}`);

    // 4 — only the region toggle removes the region entirely
  }

  /*
    Suite F — navigation must survive every ordinary control.

    It is the way back to everything else, so the panel ✕, the region toggle
    and a width change must all leave it standing. Focus mode is the single
    exception, and it is explicit, visible and reversible. This suite exists
    because the rail has now been lost twice, in two different ways.
  */
  /*
    Suite G — the navigation width change must be smooth, not a swap.

    Three things have to hold at once, and checking only the outer column
    hid the fault: the column animates, the rail wrapper animates *with* it
    (it used to snap, so the contents reflowed inside a moving container),
    and the two faces cross-fade rather than one replacing the other.
  */
  async function suiteG() {
    lines.push('nav transition'); paint();
    await settled();

    const rail = () => q('[data-side="left"] > div > div');
    const face = (sel) => q(`[data-side="left"] ${sel}`);
    const opacity = (el) => (el ? Number(getComputedStyle(el).opacity).toFixed(2) : '—');

    const seenCol = [], seenRail = [], seenFade = [];
    q('[data-od-id="nav-toggle"]').click();
    for (let i = 0; i < 8; i++) {
      await frame();
      seenCol.push(width('[data-side="left"]'));
      seenRail.push(Math.round(rail().getBoundingClientRect().width));
      seenFade.push(opacity(face('[data-od-id="nav-wide"]')));
    }

    row('column animates', new Set(seenCol).size > 2, seenCol.join(' '));
    row('rail animates with it', new Set(seenRail).size > 2, seenRail.join(' '));
    row('no snap between them', seenRail.every((w, i) => Math.abs(w - seenCol[i]) < 320),
      'wrapper tracks the column');
    row('faces cross-fade', new Set(seenFade).size > 2, seenFade.join(' '));

    await settled();
    row('both faces stay mounted', !!face('[data-od-id="nav-wide"]') && !!face('.railface:not(.wide)'),
      'stacked, not swapped');
  }

  /*
    Suite I — labels name the thing, not the widget it sits in.

    "Open Attention panel" says "panel" twice over: Attention *is* the panel.
    The same fault made a pane header read "Inspector" instead of the work item
    it was showing. A tooltip may use a widget noun only where the thing has no
    name of its own.
  */
  async function suiteI() {
    lines.push('labels name the thing'); paint();
    await ui.expandNav();
    await settled();

    const NOUNS = /\b(panel|pane|view|tab|widget|sidebar|region)\b/i;
    const titled = () => Array.from(document.querySelectorAll('[data-side="left"] .railface:not([inert]) [title], .topbar [title], [data-side] .pane-actions [title]'));

    const offenders = titled()
      .map(el => el.getAttribute('title'))
      .filter(t => t && NOUNS.test(t));

    row('no widget nouns in tooltips', offenders.length === 0,
      offenders.length ? offenders.join(' | ') : `${titled().length} tooltips clean`);

    const attention = navSwitch('Attention');
    row('a switch names its surface', attention?.getAttribute('title') === 'Close Attention',
      attention?.getAttribute('title') ?? 'missing');

    attention.click(); await settled();
    row('and follows its state', navSwitch('Attention')?.getAttribute('title') === 'Open Attention',
      navSwitch('Attention')?.getAttribute('title') ?? 'missing');

    const region = q('[data-region="left"]');
    row('a region toggle names its panel', region.getAttribute('title') === 'Show Attention',
      region.getAttribute('title'));
    row('and its pressed state follows', region.getAttribute('aria-pressed') === 'false', 'hidden → not pressed');

    navSwitch('Attention').click(); await settled();
    const close = q('[data-side="right"] [data-act="close"]');
    row('close names what closes', close?.getAttribute('title') === 'Close service-auth#5',
      close?.getAttribute('title') ?? 'missing');
    row('centre has no close', !q('[data-side="centre"] [data-act="close"]'), 'views leave by nav or tab');

    /* The right toggle hides rather than closes; its title and pressed state
       must still follow what is actually showing, named by subject. */
    const rightToggle = q('[data-region="right"]');
    row('right toggle names its subject', rightToggle.getAttribute('title') === 'Hide service-auth#5' && rightToggle.getAttribute('aria-pressed') === 'true',
      `${rightToggle.getAttribute('title')} · pressed ${rightToggle.getAttribute('aria-pressed')}`);
    rightToggle.click(); await settled();
    row('hidden → Show, not pressed', rightToggle.getAttribute('title') === 'Show service-auth#5' && rightToggle.getAttribute('aria-pressed') === 'false',
      `${rightToggle.getAttribute('title')} · pressed ${rightToggle.getAttribute('aria-pressed')}`);
    rightToggle.click(); await settled();
    row('shown again → pressed', rightToggle.getAttribute('aria-pressed') === 'true' && width('[data-side="right"]') > 100,
      `right ${width('[data-side="right"]')}px`);
  }

  async function suiteF() {
    lines.push('navigation survives'); paint();
    if (!window.__placement('attention')) await ui.togglePanel('Attention');
    await settled();

    await ui.expandNav();                                   // widest case first
    q('[data-region="left"]').click(); await settled();
    row('region toggle keeps nav', railCount() > 0 || !!q('[data-od-id="nav-wide"]'),
      `nav present · left ${width('[data-side="left"]')}px`);
    row('and it closed the panel', !window.__placement('attention'), 'attention closed');

    q('[data-region="left"]').click(); await settled();
    row('toggle restores the panel', window.__placement('attention') === 'left', 'attention back');

    await ui.expandNav();                                   // back to the rail
    q('[data-side="left"] [data-act="close"]')?.click(); await settled();
    row('panel close keeps nav', railCount() > 0, `rail items ${railCount()}`);

    const railBefore = railCount();
    q('[data-od-id="focus-toggle"]').click(); await settled();
    row('only focus hides nav', railCount() === 0, 'nav gone in focus mode');
    q('[data-od-id="focus-toggle"]').click(); await settled();
    row('and focus gives it back', railCount() === railBefore, `rail items ${railCount()}`);
  }

  /* Suite C — the nav says two different things and must never confuse them:
     which view is current, and which panels are open. */
  async function suiteC() {
    lines.push('nav grammar'); paint();
    await ui.expandNav();   // rail → full navigation
    const navBtns = () => Array.from(document.querySelectorAll('[data-side="left"] .railface:not([inert]) .nav button'));
    const byLabel = (t) => navBtns().find(b => b.textContent.startsWith(t));

    row('nav splits kinds',
      navBtns().filter(b => b.dataset.kind === 'view').length === 3 && navBtns().filter(b => b.dataset.kind === 'panel').length === 3,
      `${navBtns().filter(b => b.dataset.kind === 'view').length} views · ${navBtns().filter(b => b.dataset.kind === 'panel').length} panels`);

    row('only one current', navBtns().filter(b => b.getAttribute('aria-current') === 'page').length === 1,
      'current: ' + navBtns().filter(b => b.getAttribute('aria-current') === 'page').map(b => b.textContent.replace(/\d+$/, '')).join(','));

    // opening a panel must not touch the current view, and must not read as current
    byLabel('Flight').click(); await wait(SETTLE);
    const currents = navBtns().filter(b => b.getAttribute('aria-current') === 'page');
    row('panel ≠ current', currents.length === 1 && currents[0].dataset.kind === 'view',
      `${currents.length} current · kind ${currents[0] && currents[0].dataset.kind}`);
    /* Assert the model, not the pixels: this suite is about nav semantics, and
       the surface's declared home is what the nav promises. Whether the bottom
       region paints is suite A's job, where it is measured. */
    row('panel opens its declared slot',
      window.__placement('recorder') === 'bottom' && byLabel('Flight').getAttribute('aria-checked') === 'true',
      `at ${window.__placement('recorder')} · switch on`);

    // switching the centre moves the current marker, panel stays open
    byLabel('Runway').click(); await wait(SETTLE);
    row('current follows centre', byLabel('Runway').getAttribute('aria-current') === 'page' && byLabel('Floor').getAttribute('aria-current') !== 'page',
      'Runway current · Floor ' + (byLabel('Floor').dataset.open === 'true' ? 'open' : 'closed'));
    row('open ≠ current', byLabel('Floor').dataset.open === 'true' && byLabel('Runway').dataset.open === 'true',
      'Floor open, not current');
    row('panel survives nav', byLabel('Flight').getAttribute('aria-checked') === 'true', 'still checked');

    // and the switch toggles back off
    byLabel('Flight').click(); await wait(SETTLE);
    row('panel toggles off', byLabel('Flight').getAttribute('aria-checked') === 'false' && !window.__placement('recorder'),
      'closed · placement ' + String(window.__placement('recorder')));
  }

  /* Suite D — the nav section must follow where a surface actually is. */
  async function suiteD() {
    lines.push('placement → nav section'); paint();
    await ui.expandNav();
    const nav = (id) => document.querySelector(`[data-side="left"] .railface:not([inert]) [data-nav="${id}"]`);
    const sectionOf = (id) => { const b = nav(id); return b ? (b.closest('[data-section="secondary"]') ? 'panel' : 'primary') : 'missing'; };
    const S2 = 32; // section derivation is state-driven, so it settles fast

    /*
      Relocation itself is model behaviour and is covered by
      packages/console-model/test/placement.test.ts — move a surface, assert the
      section follows. What only a browser can check is that the derivation
      reaches the DOM: the right grammar on the right item, in the right group.
      When drag-and-drop lands, it drives the moves here.
    */
    row('separator present', !!document.querySelector('[data-side="left"] .navsep'), 'hr rendered');
    row('sections match placement', sectionOf('runway') === 'primary' && sectionOf('attention') === 'panel',
      `runway ${sectionOf('runway')} · attention ${sectionOf('attention')}`);
    row('views take radio grammar', nav('runway').dataset.kind === 'view' && !nav('runway').getAttribute('role'),
      'aria-current, no switch role');
    row('panels take switch grammar', nav('attention').getAttribute('role') === 'switch',
      'role=switch, no current marker');

    /* Opening a panel is enough to prove the derivation is live: the switch
       flips and the item stays in the Panels group. */
    nav('recorder').click(); await wait(S2);
    row('opening updates the switch', nav('recorder').getAttribute('aria-checked') === 'true'
      && window.__placement('recorder') === 'bottom', 'at bottom · switch on');
    row('and it stays a panel', sectionOf('recorder') === 'panel', 'still under Panels');

    nav('recorder').click(); await wait(S2);
    row('closing returns it home', sectionOf('recorder') === 'panel'
      && nav('recorder').getAttribute('aria-checked') === 'false' && !window.__placement('recorder'),
      'closed, switch off');
  }

  /* Suite E — theme is a setting, and the toggle actually swaps the palette. */
  async function suiteE() {
    lines.push('theme'); paint();
    const root = document.documentElement;
    row('opens dark', root.dataset.theme === 'dark',
      `theme=${root.dataset.theme} · os prefers light=${window.matchMedia('(prefers-color-scheme: light)').matches}`);
    const toggle = q('[data-od-id="theme-toggle"]');
    const swatch = () => getComputedStyle(q('[data-side="centre"] .pane')).backgroundColor;

    const before = root.dataset.theme;
    const beforeSwatch = swatch();
    row('toggle exists', !!toggle, toggle ? 'in the top bar' : 'missing');

    toggle.click(); await wait(60);
    row('theme flips', root.dataset.theme !== before, `${before} → ${root.dataset.theme}`);
    row('palette actually changes', swatch() !== beforeSwatch, `${beforeSwatch} → ${swatch()}`);
    row('glyph follows', toggle.dataset.themeNow === root.dataset.theme, 'shows the other theme');

    row('motion is on, unconditionally', q('[data-anim]').dataset.anim === 'true', 'no host signal is consulted');
    row('no motion control exists', !q('[data-od-id="motion-toggle"]'), 'nothing can switch it off');
    row('one tab style, hardcoded', !q('[data-tab="chip"]') && !q('[data-tabs]'), 'chip; no switcher');

    /* Focus mode is the fourth old preset, now a real mode: it must collapse
       every pane and give back exactly the layout it took. */
    const panes = { l: width('[data-side="left"]'), r: width('[data-side="right"]') };
    await ui.focus();
    row('focus collapses the panes', width('[data-side="left"]') < 4 && width('[data-side="right"]') < 4,
      `L ${width('[data-side="left"]')} R ${width('[data-side="right"]')}`);
    row('focus has a visible exit', q('[data-od-id="focus-toggle"]').getAttribute('aria-pressed') === 'true',
      'the control stays, pressed');

    await ui.focus();
    row('leaving restores exactly', width('[data-side="left"]') === panes.l && width('[data-side="right"]') === panes.r,
      `L ${panes.l}→${width('[data-side="left"]')} · R ${panes.r}→${width('[data-side="right"]')}`);

    toggle.click(); await wait(60);
    row('and flips back', root.dataset.theme === before, `${root.dataset.theme}`);
    toggle.click(); await wait(60); // leave it on the other theme for the screenshot
  }

  async function suiteJ() {
    const app = q('[data-od-id="app"]');
    row('opens with motion on', app.dataset.anim === 'true', 'shipped default');
    row('closed drawer is absent', !q('[data-od-id="drawer"]'), 'no hidden focus targets');
    // Shorten only after asserting the real opening state, like other suites.
    app.style.setProperty('--dur-panel', '80ms');
    q('[data-w="820"]').click(); await settled();
    const trigger = q('[data-od-id="hamburger"]');
    const centreBefore = width('[data-side="centre"]');
    trigger.focus(); trigger.click();
    const entry = [];
    for (let i = 0; i < 4; i++) {
      await frame();
      const panel = q('[data-od-id="drawer-panel"]');
      if (panel) entry.push(getComputedStyle(panel).transform);
    }
    row('drawer slides in', new Set(entry).size >= 3, `${new Set(entry).size} positions`);
    const dialog = q('[data-od-id="drawer"]');
    row('focus captured', dialog?.contains(document.activeElement), document.activeElement?.tagName);
    row('layout stays put', Math.abs(width('[data-side="centre"]') - centreBefore) <= 1, 'centre unchanged');
    const panel = q('[data-od-id="drawer-panel"]');
    const bounds = panel.getBoundingClientRect(), stage = app.getBoundingClientRect();
    row('drawer fits stage', bounds.width <= stage.width && bounds.height <= stage.height, `${Math.round(bounds.width)} × ${Math.round(bounds.height)}`);
    await Promise.all(panel.getAnimations().map(a => a.finished.catch(() => {})));
    await frame();
    document.activeElement.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));
    const exit = [];
    for (let i = 0; i < 4; i++) {
      await frame();
      if (panel.isConnected) exit.push(getComputedStyle(panel).transform);
    }
    row('drawer slides out', new Set(exit).size >= 3, `${new Set(exit).size} positions`);
    await Promise.all(panel.getAnimations().map(a => a.finished.catch(() => {})));
    // React commits the unmount, then FocusScope returns focus on a frame.
    await frame(); await frame(); await frame();
    row('Escape removes drawer', !q('[data-od-id="drawer"]'), 'exit completed');
    row('focus returns', document.activeElement === trigger, document.activeElement?.getAttribute('aria-label'));
    // Leave the actual drawer open for visual inspection after all assertions.
    trigger.click();
  }

  /* Suite K — the command palette on the shared Overlay: shortcut, ranking,
     Enter, and a picked work item reaching the inspector. */
  async function suiteK() {
    const app = q('[data-od-id="app"]');
    await settled();
    row('closed palette is absent', !q('[data-od-id="command-palette"]'), 'no hidden rows');
    app.style.setProperty('--dur-panel', '80ms');
    const trigger = q('[data-od-id="search"]');
    row('search is a trigger', trigger?.tagName === 'BUTTON' && trigger.getAttribute('aria-haspopup') === 'dialog', trigger?.tagName);
    /* A full press: React Aria triggers an item on the simulated keydown/keyup pair, not on keydown alone. */
    const key = (target, key, init = {}) => ['keydown', 'keyup'].forEach(type =>
      target.dispatchEvent(new KeyboardEvent(type, { key, bubbles: true, cancelable: true, ...init })));
    const type = (input, text) => {
      input.dispatchEvent(new InputEvent('beforeinput', { bubbles: true, cancelable: true, inputType: 'insertText', data: text }));
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(input, text);
      input.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertText', data: text }));
    };
    const rows = () => Array.from(document.querySelectorAll('.od-palette-row')).map(r => r.dataset.odId);
    // Exit motion, then React's unmount commit, then FocusScope's restore frame.
    const settle = async (panel) => { await Promise.all(panel.getAnimations().map(a => a.finished.catch(() => {}))); for (let i = 0; i < 4; i++) await frame(); };
    const where = () => { const a = document.activeElement; return a ? `${a.tagName.toLowerCase()} ${a.dataset.odId ?? ''}`.trim() : 'none'; };

    // Focus returns to wherever the operator was; start somewhere observable.
    trigger.focus();
    key(document.body, 'p', { ctrlKey: true });
    await frame(); await frame();
    row('Ctrl+P opens', !!q('[data-od-id="command-palette"]'), 'dialog present');
    let input = q('[data-od-id="command-palette-input"]');
    row('focus in field', document.activeElement === input, document.activeElement?.tagName);
    let panel = q('[data-od-id="command-palette-panel"]');
    const bounds = panel.getBoundingClientRect(), stage = app.getBoundingClientRect();
    row('palette fits stage', bounds.width <= stage.width && bounds.bottom <= stage.bottom, `${Math.round(bounds.width)} × ${Math.round(bounds.height)}`);
    await settle(panel);
    row('lists commands', rows().length >= 8 && rows()[0] === 'view:floor', rows().length + ' rows');
    row('move rows need a query', !rows().some(id => id.startsWith('move:')), 'secondary hidden');
    type(input, 'run');
    await frame(); await frame();
    row('filters and ranks', rows()[0] === 'view:runway' && rows().includes('run:run-4471'), rows().slice(0, 3).join(' '));
    row('first match focused', document.querySelector('.od-palette-row[data-focused]')?.dataset.odId === 'view:runway', 'virtual cursor');
    key(input, 'Enter');
    await frame(); await frame();
    row('Enter runs it', q('[data-side="centre"] .pane-body')?.dataset.active === 'runway', 'runway in the centre');
    await settle(panel);
    row('closes after running', !q('[data-od-id="command-palette"]'), 'exit completed');
    row('focus returns', document.activeElement === trigger, where());

    trigger.click();
    await frame(); await frame();
    input = q('[data-od-id="command-palette-input"]');
    panel = q('[data-od-id="command-palette-panel"]');
    row('reopens blank', input?.value === '' && rows()[0] === 'view:floor', 'query cleared');
    type(input, '#87');
    await frame(); await frame();
    row('finds a work item', rows().length === 1 && rows()[0] === 'item:service-portfolio-api#87', rows().join(' '));
    key(input, 'Enter');
    await frame(); await frame();
    await settle(panel);
    const head = q('[data-side="right"] .pane-title[data-subject="inspector"] .ref');
    row('inspector shows it', head?.textContent === 'service-portfolio-api#87', head?.textContent);
    row('body is the board row', !!q('[data-od-id="inspector-picked"]'), 'picked fields');

    // Leave the palette open on a query for visual inspection.
    trigger.click();
    await frame(); await frame();
    type(q('[data-od-id="command-palette-input"]'), 'attention');
  }

  /* Suite L — the inspector: select → float → pin → unpin, and a look never
     shoves the layout. */
  async function suiteL() {
    const app = q('[data-od-id="app"]');
    await settled();
    app.style.setProperty('--dur-panel', '40ms');
    const where = () => { const a = document.activeElement; return a ? `${a.tagName.toLowerCase()} ${a.dataset.item ?? a.dataset.odId ?? ''}`.trim() : 'none'; };
    const floatHead = () => q('[data-od-id="inspector-float-head"] .ref')?.textContent;
    const settleFloat = async () => {
      const p = q('[data-od-id="inspector-float-panel"]');
      if (p) await Promise.all(p.getAnimations().map(a => a.finished.catch(() => {})));
      for (let i = 0; i < 4; i++) await frame();
    };
    row('starts docked, no subject', window.__placement('inspector') === 'right' && !q('[data-side="right"] [data-act="unpin"]'), 'fixture item · no float control');
    q('[data-region="right"]').click(); await settled();
    q('[data-side="centre"] .tab:nth-child(2)')?.click(); await settled();
    const c0 = width('[data-side="centre"]'), r0 = width('[data-side="right"]');
    const card = q('[data-side="centre"] .card[data-item="service-portfolio-api#87"]');
    row('board card is selectable', card?.getAttribute('role') === 'button', card ? 'role=button' : 'missing');
    card.click(); await frame(); await frame();
    // The surface stays placed in the hidden region; what matters is that nothing docked appeared.
    row('selection floats', !!q('[data-od-id="inspector-float"]') && width('[data-side="right"]') === 0, 'overlay, not a pane');
    row('layout not shoved', width('[data-side="centre"]') === c0 && width('[data-side="right"]') === r0, `centre ${c0} · right ${r0}`);
    row('card marked selected', card.dataset.selected === 'true', 'hairline');
    row('header names it', floatHead() === 'service-portfolio-api#87', floatHead());
    row('focus not stolen', !q('[data-od-id="inspector-float"]')?.contains(document.activeElement), where());
    await settleFloat();
    q('[data-side="centre"] .card[data-item="service-auth#5"]').click(); await frame(); await frame();
    row('next click swaps subject', floatHead() === 'service-auth#5' && width('[data-side="right"]') === 0, 'still floating');
    q('[data-od-id="inspector-float"] [data-act="pin"]').click(); await settled(); await settleFloat();
    // The right region was hidden by the toggle above; pin must reveal it, not activate an invisible pane.
    row('pin docks and reveals', window.__placement('inspector') === 'right' && width('[data-side="right"]') > 100 && !q('[data-od-id="inspector-float"]'), `right ${width('[data-side="right"]')}px`);
    row('docked header names it', q('[data-side="right"] .pane-title .ref')?.textContent === 'service-auth#5', q('[data-side="right"] .pane-title .ref')?.textContent);
    const unpin = q('[data-side="right"] [data-act="unpin"]');
    row('docked pane offers float', !!unpin && /^Float /.test(unpin.getAttribute('title') || ''), unpin?.getAttribute('title'));
    unpin.click(); await settled(); await frame(); await frame();
    row('unpin floats it again', !window.__placement('inspector') && !!q('[data-od-id="inspector-float"]'), 'placement null');
    await settleFloat();
    q('[data-side="centre"] .tab:nth-child(1)')?.click(); await settled();
    const node = q('[data-side="centre"] g[data-item="all-about-money-ui#319"]');
    node?.dispatchEvent(new MouseEvent('click', { bubbles: true })); await frame(); await frame();
    row('floor node selects', floatHead() === 'all-about-money-ui#319', floatHead());
    q('[data-od-id="inspector-float"] [data-act="pin"]').focus();
    document.activeElement.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));
    await settleFloat();
    row('Escape inside closes', !q('[data-od-id="inspector-float"]'), 'dismissed');
    // Leave a floating inspector open for the capture.
    node?.dispatchEvent(new MouseEvent('click', { bubbles: true }));
  }

  /* Suite Q — the P0 decision Dialog: not dismissable, minimises to a strip,
     operations confirm and audit, resume only with a recorded justification. */
  async function suiteQ() {
    const app = q('[data-od-id="app"]');
    await settled();
    app.style.setProperty('--dur-panel', '40ms');
    const settleP0 = async () => {
      const p = q('[data-od-id="p0-panel"]');
      if (p) await Promise.all(p.getAnimations().map(a => a.finished.catch(() => {})));
      for (let i = 0; i < 4; i++) await frame();
    };
    const key = (target, k) => ['keydown', 'keyup'].forEach(t => target.dispatchEvent(new KeyboardEvent(t, { key: k, bubbles: true, cancelable: true })));
    q('[data-od-id="raise-p0"]').click(); await frame(); await frame();
    const dlg = q('[data-od-id="p0"]');
    row('raise opens an alertdialog', dlg?.getAttribute('role') === 'alertdialog', dlg?.getAttribute('role') ?? 'missing');
    await settleP0();
    row('focus captured', !!dlg && dlg.contains(document.activeElement), document.activeElement?.tagName);
    key(document.activeElement, 'Escape'); await settleP0();
    row('Escape does nothing', !!q('[data-od-id="p0"]'), 'still open');
    const scrim = q('[data-od-id="p0-backdrop"]');
    scrim.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true })); scrim.dispatchEvent(new PointerEvent('pointerup', { bubbles: true })); await settleP0();
    row('scrim click does nothing', !!q('[data-od-id="p0"]'), 'still open');
    q('[data-od-id="p0"] [data-act="resume"]').click(); await frame();
    row('resume needs justification', !!q('[data-od-id="p0-warn"]') && q('[data-od-id="p0-justification"]').getAttribute('aria-invalid') === 'true', 'refused · field flagged');
    const abort = q('[data-od-id="p0"] [data-act="abort"]');
    abort.click(); await frame();
    row('abort asks to confirm', abort.dataset.confirm === 'true' && !q('[data-od-id="p0-abort-result"]'), abort.textContent);
    abort.click(); await frame();
    row('confirmed abort is audited', !!q('[data-od-id="p0-abort-result"]') && abort.disabled, 'result shown · control spent');
  }

  /* Suite T — the floating inspector: drag by its header, position kept
     across close and reopen, clamped to the stage, a bottom sheet when narrow. */
  async function suiteT() {
    const app = q('[data-od-id="app"]');
    await settled();
    app.style.setProperty('--dur-panel', '40ms');
    const settleFloat = async () => {
      const p = q('[data-od-id="inspector-float-panel"]');
      if (p) await Promise.all(p.getAnimations().map(a => a.finished.catch(() => {})));
      for (let i = 0; i < 4; i++) await frame();
    };
    const P = (type, target, x, y) => target.dispatchEvent(new PointerEvent(type, { bubbles: true, cancelable: true, clientX: x, clientY: y, button: 0, pointerId: 1, pointerType: 'mouse' }));
    // Setup is kept short so the rows land inside the capture window: hide the
    // right region, select a floor node (no tab switch needed).
    q('[data-region="right"]').click(); await wait(110);
    const node = q('[data-side="centre"] g[data-item="all-about-money-ui#319"]');
    const pick = () => node.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    pick(); await frame(); await frame(); await settleFloat();
    let panel = q('[data-od-id="inspector-float-panel"]');
    const head = q('[data-od-id="inspector-float-head"]');
    const before = panel.getBoundingClientRect();
    row('header is a drag handle', head?.dataset.drag === 'off', `data-drag ${head?.dataset.drag}`);
    const hx = before.left + 40, hy = before.top + 20;
    P('pointerdown', head, hx, hy); await frame();
    P('pointermove', window, hx - 200, hy + 60); await frame(); await frame();
    row('drags while held', head.dataset.drag === 'on', 'grabbing');
    P('pointerup', window, hx - 200, hy + 60); await frame(); await frame();
    const after = panel.getBoundingClientRect();
    row('moves with the pointer', Math.round(before.left - after.left) === 200 && Math.round(after.top - before.top) === 60, `dx ${Math.round(after.left - before.left)} · dy ${Math.round(after.top - before.top)}`);
    P('pointerdown', head, after.left + 40, after.top + 20); await frame();
    P('pointermove', window, after.left + 40 - 5000, after.top + 20 - 5000); await frame(); await frame();
    P('pointerup', window, after.left + 40 - 5000, after.top + 20 - 5000); await frame(); await frame();
    const clamped = panel.getBoundingClientRect(), stage = app.getBoundingClientRect();
    row('stays on the stage', clamped.left >= stage.left && clamped.top >= stage.top + 40, `left ${Math.round(clamped.left - stage.left)} · top ${Math.round(clamped.top - stage.top)}`);
  }

  /* Suite AB — the 2026-09-12 polish: bottom toggle state, sliding selection
     on rail and tabs, no coloured edges, icon buttons on the P0 strip. */
  async function suiteAB() {
    const app = q('[data-od-id="app"]');
    await wait(150);
    const sample = async (sel, prop, n = 6) => { const seen = []; for (let i = 0; i < n; i++) { await frame(); const el = q(sel); seen.push(el ? Math.round(parseFloat(getComputedStyle(el)[prop])) : -1); } return seen; };
    const tl0 = Math.round(parseFloat(getComputedStyle(q('[data-od-id="tabs-highlight-centre"]')).left));
    q('[data-side="centre"] .tab[data-tab="runway"]').click();
    const tabSeen = await sample('[data-od-id="tabs-highlight-centre"]', 'left');
    const thl = q('[data-od-id="tabs-highlight-centre"]');
    row('tab selection glides', new Set(tabSeen).size > 2 && tabSeen[tabSeen.length - 1] > tl0, `left ${tl0} → ${tabSeen.join(' ')} · anims ${thl.getAnimations().length}`);
    const bottom = q('[data-region="bottom"]');
    row('bottom toggle starts unpressed', bottom.getAttribute('aria-pressed') === 'false', bottom.getAttribute('aria-pressed'));
    bottom.click(); await wait(140);
    row('opens → pressed', bottom.getAttribute('aria-pressed') === 'true' && window.__placement('recorder') === 'bottom', `pressed ${bottom.getAttribute('aria-pressed')} · recorder at ${window.__placement('recorder')}`);
    bottom.click(); await wait(140);
    row('closes → unpressed', bottom.getAttribute('aria-pressed') === 'false', bottom.getAttribute('aria-pressed'));
    const hl0 = Math.round(parseFloat(getComputedStyle(q('[data-od-id="rail-highlight"]')).top));
    q('[data-rail="board"]').click();
    const railSeen = await sample('[data-od-id="rail-highlight"]', 'top');
    row('rail selection glides', new Set(railSeen).size > 2 && railSeen[railSeen.length - 1] > hl0, `top ${hl0} → ${railSeen.join(' ')}`);
    row('no marker bars', !document.querySelector('.railbtn .mk, .nav button .mk') || getComputedStyle(q('.railbtn .mk')).display === 'none', 'display none');

    q('[data-side="centre"] .tab[data-tab="board"]').click(); await wait(120);
    const card = q('[data-side="centre"] .card');
    row('cards carry a dot, not an edge', getComputedStyle(card).borderLeftWidth === '0px' && !!card.querySelector('.ref .dot'), `border-left ${getComputedStyle(card).borderLeftWidth}`);
    const you = q('[data-side="left"] .att.you');
    row('needs-you is a halo on the dot', getComputedStyle(you).boxShadow === 'none' && getComputedStyle(you.querySelector('.dot')).boxShadow !== 'none', 'halo present');
    q('[data-od-id="raise-p0"]').click(); await wait(80);
    const min = q('[data-od-id="p0"] [data-act="minimize"]');
    row('minimise is an icon button', min?.classList.contains('ibtn') && !!min.querySelector('svg') && !!min.title, min?.title ?? 'missing');
    min.click(); await wait(120);
    const open = q('[data-od-id="critical-open"]');
    row('strip open is an icon button', open?.classList.contains('ibtn') && !!open.querySelector('svg') && !!open.title, open?.title ?? 'missing');
    row('strip is a neutral surface', getComputedStyle(q('[data-od-id="critical-banner"]')).backgroundColor === getComputedStyle(q('.pill')).backgroundColor, 'surface-2');
    row('floor shares the pane surface', !getComputedStyle(q('.canvas') || document.body).backgroundImage.includes('gradient'), 'no gradient');
  }

  /* Suite AA — breadcrumb: the inspector shows where its item came from; a
     level is a one-click jump back. */
  async function suiteAA() {
    await wait(150);
    q('[data-side="centre"] .tab:nth-child(2)').click(); await wait(120);
    q('[data-side="centre"] .card[data-item="service-portfolio-api#87"]').click(); await wait(120);
    const crumbs = Array.from(document.querySelectorAll('[data-side="right"] [data-od-id="crumbs"] li')).map(l => l.textContent.trim());
    row('path shows source and column', crumbs.join(' › ') === 'Board › Backlog › service-portfolio-api#87', crumbs.join(' › ') || 'no crumbs');
    row('current level is not a link', q('[data-side="right"] [data-od-id="crumbs"] li[aria-current="location"] button') === null, 'span, not button');
    q('[data-side="centre"] .tab:nth-child(1)').click(); await wait(120);
    q('[data-side="right"] [data-od-id="crumbs"] [data-crumb="Board"]').click(); await wait(140);
    row('a level jumps back', q('[data-side="centre"] .pane-body')?.dataset.active === 'board', q('[data-side="centre"] .pane-body')?.dataset.active);
    q('[data-rail="attention"]')?.click(); await wait(100);
    q('[data-side="left"] .att[data-item="all-about-money-ui#319"]')?.click(); await wait(120);
    const c2 = Array.from(document.querySelectorAll('[data-side="right"] [data-od-id="crumbs"] li')).map(l => l.textContent.trim());
    row('attention items carry their own path', c2.join(' › ') === 'Attention › Ready › all-about-money-ui#319', c2.join(' › ') || 'no crumbs');
  }

  /* Suite Z — drag-rearrange: hold a tab, zones appear, the pointed one
     lights, dropping moves the surface through the reducer; Escape cancels. */
  async function suiteZ() {
    const app = q('[data-od-id="app"]');
    await wait(150);
    const P = (type, target, x, y) => target.dispatchEvent(new PointerEvent(type, { bubbles: true, cancelable: true, clientX: x, clientY: y, button: 0, pointerId: 1, pointerType: 'mouse' }));
    const tab = q('[data-side="centre"] .tab[data-tab="runway"]');
    const t = tab.getBoundingClientRect(), right = q('[data-side="right"]').getBoundingClientRect();
    P('pointerdown', tab, t.left + 20, t.top + 10); await frame();
    P('pointermove', window, t.left + 24, t.top + 12); await frame();
    row('a nudge is not a drag', !q('[data-od-id="dropzones"]'), 'under 6px');
    P('pointermove', window, right.left + 60, right.top + 200); await frame(); await frame();
    const zones = document.querySelectorAll('[data-od-id="dropzones"] .dropzone');
    row('zones appear once held', zones.length === 4, `${zones.length} zones`);
    row('pointed zone lights up', q('.dropzone[data-over="true"]')?.dataset.zone === 'right', q('.dropzone[data-over="true"]')?.dataset.zone ?? 'none');
    row('own region reads "here"', q('.dropzone[data-zone="centre"]')?.textContent === 'here' && q('.dropzone[data-zone="centre"]')?.dataset.allowed === 'false', q('.dropzone[data-zone="centre"]')?.textContent);
    row('ghost carries the label', q('.drag-ghost')?.textContent === 'Runway', q('.drag-ghost')?.textContent ?? 'none');
    P('pointerup', window, right.left + 60, right.top + 200); await wait(140);
    row('drop moves it', window.__placement('runway') === 'right' && !q('[data-od-id="dropzones"]'), `runway at ${window.__placement('runway')}`);
    row('and it becomes the active tab there', q('[data-side="right"] .tab[aria-selected="true"]')?.dataset.tab === 'runway', q('[data-side="right"] .tab[aria-selected="true"]')?.dataset.tab ?? 'none');
    // Escape cancels a drag in flight.
    const back = q('[data-side="right"] .tab[data-tab="runway"]');
    const b = back.getBoundingClientRect(), centre = q('[data-side="centre"]').getBoundingClientRect();
    P('pointerdown', back, b.left + 20, b.top + 10); await frame();
    P('pointermove', window, centre.left + 200, centre.top + 200); await frame(); await frame();
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true })); await frame();
    P('pointerup', window, centre.left + 200, centre.top + 200); await wait(120);
    row('Escape cancels', window.__placement('runway') === 'right' && !q('[data-od-id="dropzones"]'), `runway still at ${window.__placement('runway')}`);
    // Leave a drag in flight for the capture.
    const again = q('[data-side="right"] .tab[data-tab="runway"]');
    const a = again.getBoundingClientRect();
    P('pointerdown', again, a.left + 20, a.top + 10); await frame();
    P('pointermove', window, centre.left + 300, centre.top + 300); await frame(); await frame();
    row('zones showing for the capture', !!q('[data-od-id="dropzones"]') && q('.dropzone[data-over="true"]')?.dataset.zone === 'centre', q('.dropzone[data-over="true"]')?.dataset.zone ?? 'none');
  }

  /* Suite Y — per-view layout: each view remembers the side panes it was last
     seen with; a first visit keeps what is open. */
  async function suiteY() {
    await wait(150);
    const tab = (n) => { q(`[data-side="centre"] .tab:nth-child(${n})`).click(); };
    tab(2); await wait(120);
    row('first visit keeps the panes', window.__placement('inspector') === 'right' && q('[data-side="centre"] .pane-body')?.dataset.active === 'board', 'Board · inspector still right');
    q('[data-rail="logs"]').click(); await wait(120);
    row('Board gets run output', window.__placement('logs') === 'right', `logs at ${window.__placement('logs')}`);
    tab(1); await wait(160);
    row('Floor never had it', q('[data-side="centre"] .pane-body')?.dataset.active === 'floor' && window.__placement('logs') === null && window.__placement('inspector') === 'right', `logs at ${window.__placement('logs')} · inspector at ${window.__placement('inspector')}`);
    tab(2); await wait(160);
    row('Board gets it back', window.__placement('logs') === 'right' && q('[data-side="right"] .tab[aria-selected="true"]')?.dataset.tab === 'logs', `logs at ${window.__placement('logs')} · active ${q('[data-side="right"] .tab[aria-selected="true"]')?.dataset.tab}`);
    q('[data-side="right"] [data-act="close"]').click(); await wait(120);
    tab(1); await wait(160);
    row('closing on Board leaves Floor alone', window.__placement('inspector') === 'right', `inspector at ${window.__placement('inspector')}`);
  }

  /* Suite W — keyboard parity: tab strips with roving focus, keyboard resize on
     grips, Alt+n view jumps, and a top bar that does not overflow when narrow. */
  async function suiteW() {
    const app = q('[data-od-id="app"]');
    await wait(150);
    const key = (target, k, init = {}) => target.dispatchEvent(new KeyboardEvent('keydown', { key: k, bubbles: true, cancelable: true, ...init }));
    const strip = q('[data-side="centre"] .tabs');
    row('centre strip is a tablist', strip?.getAttribute('role') === 'tablist', strip?.getAttribute('role') ?? 'missing');
    const stops = Array.from(strip.querySelectorAll('.tab')).filter(t => t.tabIndex === 0).length;
    row('one tab stop per strip', stops === 1, `${stops} focusable of ${strip.querySelectorAll('.tab').length}`);
    strip.querySelector('.tab[aria-selected="true"]').focus();
    key(document.activeElement, 'ArrowRight'); await frame(); await frame();
    row('ArrowRight moves and activates', q('[data-side="centre"] .pane-body')?.dataset.active === 'board' && document.activeElement?.dataset.tab === 'board', `active ${q('[data-side="centre"] .pane-body')?.dataset.active} · focus ${document.activeElement?.dataset.tab}`);
    key(window, '3', { altKey: true }); await frame(); await frame();
    row('Alt+3 jumps to the third view', q('[data-side="centre"] .pane-body')?.dataset.active === 'runway', q('[data-side="centre"] .pane-body')?.dataset.active);
    const grip = q('[data-od-id="grip-right"]');
    row('grip is a focusable separator', grip?.getAttribute('role') === 'separator' && grip.tabIndex === 0, grip?.getAttribute('aria-label') ?? 'missing');
    const r0 = width('[data-side="right"]');
    grip.focus(); key(grip, 'ArrowLeft'); key(grip, 'ArrowLeft'); await wait(120);
    row('arrows resize the pane', width('[data-side="right"]') === r0 + 32, `${r0} → ${width('[data-side="right"]')}`);
    q('[data-w="820"]').click(); await wait(160);
    const bar = q('.topbar');
    // One row: the 44px navigation trigger plus the bar's padding is 60px.
    row('narrow top bar fits', bar.scrollWidth <= bar.clientWidth + 1 && bar.getBoundingClientRect().height <= 64, `${bar.scrollWidth} of ${bar.clientWidth} · ${Math.round(bar.getBoundingClientRect().height)}px tall`);
    const pills = Array.from(document.querySelectorAll('.topbar .pill')).map(p => Math.round(p.getBoundingClientRect().height));
    row('pills stay one line', pills.every(h => h <= 26), pills.join(' '));
  }

  /* Suite V — narrow viewport: with the side panes at zero width a selection
     floats as a bottom sheet, full width, no drag handle. */
  async function suiteV() {
    const app = q('[data-od-id="app"]');
    await wait(120);
    app.style.setProperty('--dur-panel', '40ms');
    q('[data-w="820"]').click(); await wait(160);
    row('narrow: no side width', width('[data-side="right"]') === 0 && q('#bpnote').textContent.startsWith('sm'), q('#bpnote').textContent.split(' · ')[0]);
    const node = q('[data-side="centre"] g[data-item="all-about-money-ui#319"]');
    node.dispatchEvent(new MouseEvent('click', { bubbles: true })); await frame(); await frame();
    const p = q('[data-od-id="inspector-float-panel"]');
    if (p) await Promise.all(p.getAnimations().map(a => a.finished.catch(() => {})));
    await frame(); await frame();
    const s = p?.getBoundingClientRect(), st = app.getBoundingClientRect();
    row('selection floats as a sheet', !!s && s.width >= st.width - 2 && Math.abs(s.bottom - st.bottom) < 2, s ? `${Math.round(s.width)} of ${Math.round(st.width)} wide · ${Math.round(st.bottom - s.bottom)} above the edge` : 'no float');
    row('sheet has no drag handle', !!p && !q('[data-od-id="inspector-float-head"]')?.dataset.drag, 'data-drag absent');
    row('subject shown', q('[data-od-id="inspector-float-head"] .ref')?.textContent === 'all-about-money-ui#319', q('[data-od-id="inspector-float-head"] .ref')?.textContent ?? 'none');
  }

  /* Suite U — the tail of T: the position survives close and reopen; at a
     narrow width the float is a bottom sheet with no drag handle. */
  async function suiteU() {
    const app = q('[data-od-id="app"]');
    // No boot settle here: this suite compares rects taken at the same moment.
    await wait(120);
    app.style.setProperty('--dur-panel', '40ms');
    const settleFloat = async () => {
      const p = q('[data-od-id="inspector-float-panel"]');
      if (p) await Promise.all(p.getAnimations().map(a => a.finished.catch(() => {})));
      for (let i = 0; i < 4; i++) await frame();
    };
    const P = (type, target, x, y) => target.dispatchEvent(new PointerEvent(type, { bubbles: true, cancelable: true, clientX: x, clientY: y, button: 0, pointerId: 1, pointerType: 'mouse' }));
    q('[data-region="right"]').click(); await wait(110);
    const node = q('[data-side="centre"] g[data-item="all-about-money-ui#319"]');
    const pick = () => node.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    pick(); await frame(); await frame(); await settleFloat();
    let panel = q('[data-od-id="inspector-float-panel"]');
    const head = q('[data-od-id="inspector-float-head"]');
    const r0 = panel.getBoundingClientRect();
    P('pointerdown', head, r0.left + 40, r0.top + 20); await frame();
    P('pointermove', window, r0.left + 40 - 160, r0.top + 20 + 90); await frame();
    P('pointerup', window, r0.left + 40 - 160, r0.top + 20 + 90); await frame(); await frame();
    const kept = panel.getBoundingClientRect();
    q('[data-od-id="inspector-float"] [data-act="close"]').click(); await settleFloat();
    pick(); await frame(); await frame(); await settleFloat();
    panel = q('[data-od-id="inspector-float-panel"]');
    const re = panel.getBoundingClientRect();
    row('position remembered', Math.abs(re.left - kept.left) < 2 && Math.abs(re.top - kept.top) < 2, `left ${Math.round(re.left)} vs ${Math.round(kept.left)} · top ${Math.round(re.top)} vs ${Math.round(kept.top)}`);
    q('[data-w="820"]').click(); await wait(110); await settleFloat();
    panel = q('[data-od-id="inspector-float-panel"]');
    const s = panel?.getBoundingClientRect(), st = app.getBoundingClientRect();
    row('narrow → bottom sheet', !!s && s.width >= st.width - 2 && Math.abs(s.bottom - st.bottom) < 2, s ? `${Math.round(s.width)} wide · ${Math.round(st.width)} stage · gap below ${Math.round(st.bottom - s.bottom)}` : 'no panel');
    row('sheet has no drag handle', !q('[data-od-id="inspector-float-head"]')?.dataset.drag, 'data-drag absent');
  }

  /* Suite S — anchored Popovers in the top bar: the environment menu and the
     needs-you list, whose rows select the item for the inspector. */
  async function suiteS() {
    await settled();
    q('[data-od-id="app"]').style.setProperty('--dur-panel', '40ms');
    const envTrigger = q('[data-od-id="env-scope-trigger"]');
    row('env pill is a button', envTrigger?.tagName === 'BUTTON' && ['true', 'menu'].includes(envTrigger.getAttribute('aria-haspopup')), `aria-haspopup ${envTrigger?.getAttribute('aria-haspopup') ?? 'missing'}`);
    envTrigger.click(); await frame(); await frame(); await wait(60);
    const menu = q('[data-od-id="env-scope"]');
    row('menu opens anchored', !!menu && menu.getAttribute('role') === 'menu', menu?.getAttribute('role') ?? 'missing');
    const pop = q('[data-od-id="env-scope-popover"]');
    const tb = envTrigger.getBoundingClientRect(), pb = pop?.getBoundingClientRect();
    row('sits below its trigger', !!pb && pb.top >= tb.bottom && pb.right <= window.innerWidth, pb ? `top ${Math.round(pb.top)} vs trigger bottom ${Math.round(tb.bottom)}` : 'no popover');
    row('current choice marked', q('[data-od-id="env-scope-prod"]')?.getAttribute('aria-checked') === 'true', 'prod');
    q('[data-od-id="env-scope-staging"]').dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, pointerId: 1, pointerType: 'mouse', button: 0 }));
    q('[data-od-id="env-scope-staging"]')?.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, pointerId: 1, pointerType: 'mouse', button: 0 }));
    q('[data-od-id="env-scope-staging"]')?.click();
    await wait(120);
    row('choosing closes and updates the pill', !q('[data-od-id="env-scope"]') && /staging/.test(envTrigger.textContent), envTrigger.textContent.trim());
    const attn = q('[data-od-id="needs-you-trigger"]');
    attn.click(); await frame(); await frame(); await wait(60);
    row('needs-you opens a dialog', q('[data-od-id="needs-you"]')?.getAttribute('role') === 'dialog', String(document.querySelectorAll('[data-od-id^="needs-you-"] .od-row, .od-popover .od-row').length) + ' rows');
    q('[data-od-id="needs-you-infra-aws-bootstrap#10"]').click(); await wait(120);
    row('a row selects the item', !q('[data-od-id="needs-you"]') && q('[data-side="right"] .pane-title .ref')?.textContent === 'infra-aws-bootstrap#10', q('[data-side="right"] .pane-title .ref')?.textContent ?? 'no subject');
    // Leave the needs-you list open for the capture.
    attn.click();
  }

  /* Suite R — the tail of Q: minimise to the strip, reopen, justify, resume. */
  async function suiteR() {
    const app = q('[data-od-id="app"]');
    await settled();
    app.style.setProperty('--dur-panel', '40ms');
    const settleP0 = async () => {
      const p = q('[data-od-id="p0-panel"]');
      if (p) await Promise.all(p.getAnimations().map(a => a.finished.catch(() => {})));
      for (let i = 0; i < 4; i++) await frame();
    };
    q('[data-od-id="raise-p0"]').click(); await settleP0();
    q('[data-od-id="p0"] [data-act="minimize"]').click(); await settleP0();
    row('minimise leaves a strip', !q('[data-od-id="p0"]') && !!q('[data-od-id="critical-banner"]'), 'pause still in force');
    q('[data-od-id="critical-open"]').click(); await frame(); await frame();
    row('strip reopens the dialog', !!q('[data-od-id="p0"]') && !q('[data-od-id="critical-banner"]'), 'dialog back');
    await settleP0();
    const ta = q('[data-od-id="p0-justification"]');
    Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set.call(ta, 'Token revoked under PB-07; prod path verified clean.');
    ta.dispatchEvent(new Event('input', { bubbles: true })); await frame();
    row('typing clears the flag', ta.getAttribute('aria-invalid') === 'false' && !q('[data-od-id="p0-warn"]'), 'valid');
    q('[data-od-id="p0"] [data-act="resume"]').click(); await settleP0();
    row('resume resolves', !q('[data-od-id="p0"]') && !q('[data-od-id="critical-banner"]'), 'closed · no strip');
    // Leave the dialog open for the capture.
    q('[data-od-id="raise-p0"]').click();
  }

  /* Suite M — the tail of L, split so it fits the capture window: the docked
     inspector floats again, a floor node selects, Escape inside closes. */
  async function suiteM() {
    const app = q('[data-od-id="app"]');
    await settled();
    app.style.setProperty('--dur-panel', '40ms');
    const floatHead = () => q('[data-od-id="inspector-float-head"] .ref')?.textContent;
    const settleFloat = async () => {
      const p = q('[data-od-id="inspector-float-panel"]');
      if (p) await Promise.all(p.getAnimations().map(a => a.finished.catch(() => {})));
      for (let i = 0; i < 4; i++) await frame();
    };
    // Docked at start: a click swaps the docked subject and offers Float.
    const node = q('[data-side="centre"] g[data-item="all-about-money-ui#319"]');
    row('floor node is selectable', node?.getAttribute('role') === 'button', node ? 'role=button' : 'missing');
    node.dispatchEvent(new MouseEvent('click', { bubbles: true })); await frame(); await frame();
    row('docked: click swaps subject', q('[data-side="right"] .pane-title .ref')?.textContent === 'all-about-money-ui#319' && !q('[data-od-id="inspector-float"]'), 'no float while docked');
    const unpin = q('[data-side="right"] [data-act="unpin"]');
    unpin?.click(); await settled(); await frame(); await frame();
    row('unpin floats it', !window.__placement('inspector') && floatHead() === 'all-about-money-ui#319', `right ${width('[data-side="right"]')}px`);
    await settleFloat();
    q('[data-side="centre"] g[data-item="service-portfolio-api#195"]')?.dispatchEvent(new MouseEvent('click', { bubbles: true })); await frame(); await frame();
    row('floor node swaps subject', floatHead() === 'service-portfolio-api#195', floatHead());
    const pinBtn = q('[data-od-id="inspector-float"] [data-act="pin"]');
    pinBtn.focus();
    const esc = new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true });
    pinBtn.dispatchEvent(esc);
    await frame(); await frame();
    const exiting = !!q('[data-od-id="inspector-float-panel"][data-phase="exiting"]');
    await settleFloat();
    row('Escape inside closes', !q('[data-od-id="inspector-float"]'), `handled ${esc.defaultPrevented} · exiting ${exiting} · focused ${document.activeElement === pinBtn}`);
    row('focus handed back', document.activeElement !== document.body, document.activeElement?.tagName);
    // Leave a floating inspector open for the capture.
    node.dispatchEvent(new MouseEvent('click', { bubbles: true }));
  }

  /* Orb helpers: synthetic pointers on the real component. */
  const orbEl = () => q('[data-od-id="orb"]');
  const centreOf = (el) => { const r = el.getBoundingClientRect(); return { x: r.left + r.width / 2, y: r.top + r.height / 2 }; };
  const pe = (el, type, x, y) => el.dispatchEvent(new PointerEvent(type, { bubbles: true, cancelable: true, clientX: x, clientY: y, button: 0, buttons: 1, pointerId: 1, pointerType: 'mouse', isPrimary: true }));
  const pressOrb = async () => { const o = orbEl(); const c = centreOf(o); pe(o, 'pointerdown', c.x, c.y); await frame(); pe(o, 'pointerup', c.x, c.y); await frame(); await wait(20); };
  const ringLabels = () => Array.from(document.querySelectorAll('.od-orb-item .od-orb-label')).map(e => e.textContent);
  const pick = async (id) => { q(`[data-od-id="orb-item-${id}"]`)?.dispatchEvent(new MouseEvent('click', { bubbles: true })); await frame(); await wait(20); };
  /* Pane motion shortened so the layout settles inside the capture window; the ring's own birth uses __MOTION_MS. */
  const quickPanes = () => { const app = q('[data-od-id="app"]'); app.style.setProperty('--dur-panel', '40ms'); app.style.setProperty('--dur-transform', '60ms'); };
  /* The view the nav marks current, by id, whichever face the nav is wearing. */
  const currentNav = () => q('[data-side="left"] .railbtn[aria-current="page"]')?.dataset.rail
    ?? q('[data-side="left"] .nav [aria-current="page"]')?.dataset.nav;
  const activeTab = () => q('[data-side="centre"] [role="tab"][aria-selected="true"]')?.textContent;

  /* AC: the orb in the shell — the ring follows the nav. */
  async function suiteAC() {
    quickPanes(); await settled();
    const s = q('[data-od-id="app"]').getBoundingClientRect();
    const c = centreOf(orbEl());
    row('orb at the home corner', c.x > s.right - 60 && c.y > s.bottom - 60, `${Math.round(c.x)},${Math.round(c.y)} in ${Math.round(s.right)}×${Math.round(s.bottom)}`);
    row('below the P0 dialog', Number(getComputedStyle(q('[data-od-id="orb-layer"]')).zIndex) < 950, `z ${getComputedStyle(q('[data-od-id="orb-layer"]')).zIndex}`);
    await pressOrb();
    row('ring from the model', ringLabels().join('|') === 'Ask|Voice|Commands|Go to|Here', ringLabels().join('|'));
    await pick('go');
    row('Go to lists the views', ringLabels().join('|') === 'Back|Floor|Board|Runway', ringLabels().join('|'));
    const marked = document.querySelectorAll('.od-orb-item[data-current="true"]');
    row('current view marked', marked.length === 1 && marked[0].dataset.odId === `orb-item-view:${currentNav()}`, `${marked[0]?.dataset.odId} · nav says ${currentNav()}`);
    await pick('view:runway'); await settled();
    row('ring navigates', currentNav() === 'runway' && !q('[data-od-id="orb-radial"]'), `nav says ${currentNav()} · tab ${activeTab()}`);
    row('focus back on orb', document.activeElement === orbEl(), document.activeElement?.dataset.odId);
  }

  /* AE: with the navigation hidden, the orb is the navigation. */
  async function suiteAE() {
    quickPanes(); await settled();
    q('[title="Focus mode"]').click(); await settled();
    row('focus mode hides the rail', width('[data-side="left"]') === 0 && !q('[data-side="left"] .railbtn'), `left ${width('[data-side="left"]')}px`);
    await pressOrb();
    row('orb becomes the nav', ringLabels().join('|') === 'Ask|Voice|Commands|Floor|Board|Runway|Panels|Here', ringLabels().join('|'));
    await pick('view:board'); await settled();
    row('navigates without a rail', window.__placement('board') === 'centre' && (activeTab() ?? '').includes('Board'), `tab ${activeTab()}`);
  }

  /* AG: the panels submenu carries the way back to the nav; Inspect from the ring docks. */
  async function suiteAG() {
    quickPanes(); await settled();
    // At the smallest size the nav is a drawer, so the ring carries the views there too.
    q('[data-od-id="lab"] [data-w="820"]').click(); await settled();
    await pressOrb();
    row('smallest size: orb is the nav', ringLabels().slice(3, 6).join('|') === 'Floor|Board|Runway', ringLabels().join('|'));
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' })); await frame();
    q('[data-od-id="lab"] [data-w="0"]').click(); await settled();
    // The ring follows the model at once; only the rail's width needs to settle.
    q('[title="Focus mode"]').click(); await frame();
    await pressOrb(); await pick('panels');
    row('Panels submenu in focus mode', ringLabels().join('|') === 'Back|Close Attention|Open Flight recorder|Open Run output', ringLabels().join('|'));
    row('no nav entry on the ring', !q('[data-od-id="orb-item-nav:show"]') && !q('[data-od-id="orb-item-nav:hide"]'), '');
    await pick('__back'); window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' })); await settled();
    row('Esc leaves focus, rail back', width('[data-side="left"]') > 0 && !!q('[data-side="left"] .railbtn'), `left ${width('[data-side="left"]')}px`);
  }

  /* AH: Inspect from the ring docks the selected item. */
  async function suiteAH() {
    quickPanes(); await settled();
    q('[data-side="left"] .att[data-item="all-about-money-ui#319"]')?.click(); await frame(); await frame();
    await pressOrb(); await pick('here'); await pick('inspect:all-about-money-ui#319'); await settled();
    row('Inspect docks the item', window.__placement('inspector') === 'right' && q('[data-side="right"] .pane-title .ref')?.textContent === 'all-about-money-ui#319', `inspector at ${window.__placement('inspector')}`);
    row('ring closed, focus on orb', !q('[data-od-id="orb-radial"]') && document.activeElement === orbEl(), document.activeElement?.dataset.odId);
  }

  /* AF: the orb keeps its place as an offset from its corner, through the port and through a resize. */
  async function suiteAF() {
    quickPanes(); await settled();
    const c = centreOf(orbEl()); const s = q('[data-od-id="app"]').getBoundingClientRect();
    pe(orbEl(), 'pointerdown', c.x, c.y); await frame(); pe(orbEl(), 'pointermove', c.x - 300, c.y - 200); await wait(110); pe(orbEl(), 'pointerup', c.x - 300, c.y - 200); await frame();
    const saved = JSON.parse(store.read(ORB_KEY) || '{}');
    row('place remembered', saved.dx === -300 && saved.dy === -200, `dx ${saved.dx} · dy ${saved.dy}`);
    const now = centreOf(orbEl());
    row('orb moved there', Math.abs(now.x - (c.x - 300)) < 2 && Math.abs(now.y - (c.y - 200)) < 2, `${Math.round(now.x)},${Math.round(now.y)}`);
    // Narrow the stage from the lab bar: the orb should keep its offset from the new corner.
    q('[data-od-id="lab"] [data-w="1280"]').click(); await settled(); await frame(); await frame();
    const s2 = q('[data-od-id="app"]').getBoundingClientRect(); const c2 = centreOf(orbEl());
    row('stage narrowed', s2.width < s.width - 100, `${Math.round(s.width)} → ${Math.round(s2.width)}`);
    row('offset kept from the corner', Math.abs((s2.right - c2.x) - (s.right - (c.x - 300))) < 2, `${Math.round(s2.right - c2.x)} from the right, was ${Math.round(s.right - (c.x - 300))}`);
  }

  /* AD: context on the ring, the assistant panel, and the remembered place. */
  async function suiteAD() {
    quickPanes(); await settled();
    q('[data-side="left"] .att[data-item="all-about-money-ui#319"]')?.click(); await frame(); await frame();
    await pressOrb();
    row('selection names the context', ringLabels().pop() === 'all-about-money-ui#319', ringLabels().join('|'));
    await pick('here');
    row('context intents', ringLabels().join('|') === 'Back|Inspect all-about-money-ui#319|Ask about all-about-money-ui#319|Ask about Floor', ringLabels().join('|'));
    await pick('about:all-about-money-ui#319'); await wait(60);
    row('Ask about opens the assistant', q('[data-od-id="assistant-about"]')?.textContent === 'About all-about-money-ui#319' && !!q('[data-od-id="assistant-input"]', ''), q('[data-od-id="assistant-about"]')?.textContent);
    row('honest empty state', /not connected|No assistant/i.test(q('[data-od-id="assistant"]')?.textContent ?? ''), '');
    q('[data-od-id="assistant-mode-voice"]').click(); await frame();
    row('voice is one press away', !!q('[data-od-id="assistant"] .voice') && !q('[data-od-id="assistant-input"]'), '');
    q('[data-od-id="orb-panel-close"]').click(); await frame();
    row('close', orbEl().getAttribute('aria-expanded') === 'false', '');
  }

  (async () => {
    // width assertions must not race the boot animation; structural ones needn't wait
    await wait(SUITE === 'D' ? 110 : 200);
    /* No forcing. The harness previously turned motion on before every suite,
       which meant the animation tests passed while the delivered artifact
       opened with animation off. A test that works around the default is not
       testing the product. Suites assert the shipped state. */
    /* 'N' is a look, not a check: expand navigation and stop, so the wide
       state can be screenshotted without hand-driving the UI. */
    if (SUITE === 'N') { await ui.expandNav(); lines.push('navigation expanded'); paint(); return; }
    /* 'P' is also a look: open the palette and stop, so its resting state can be screenshotted. */
    if (SUITE === 'P') { q('[data-od-id="search"]').click(); lines.push('palette open'); paint(); return; }
    /* 'O' is a look: open the orb's ring and stop. */
    if (SUITE === 'O') { box.remove(); await settled(); await pressOrb(); return; }
    /* 'X' measures: the gaps between neighbouring panes, in CSS px, so a
       "looks off" can be answered with numbers rather than a squint. */
    if (SUITE === 'X') {
      q('[data-region="bottom"]').click(); await settled(); await wait(120);
      const r = (s) => q(s)?.getBoundingClientRect();
      const rail = r('[data-side="left"] .rail'), left = r('[data-side="left"] .pane'), centre = r('[data-side="centre"] .pane');
      const right = r('[data-side="right"] .pane'), bottom = r('[data-od-id="bottom"] .pane'), top = r('.topbar');
      const g = (a, b) => (a && b) ? Math.round((b - a) * 10) / 10 : 'n/a';
      lines.push(`rail → left pane      ${g(rail?.right, left?.left)}`);
      lines.push(`left → centre         ${g(left?.right, centre?.left)}`);
      lines.push(`centre → right        ${g(centre?.right, right?.left)}`);
      lines.push(`topbar → pane top     ${g(top?.bottom, centre?.top)}`);
      lines.push(`centre → bottom pane  ${g(centre?.bottom, bottom?.top)}`);
      lines.push(`pane bottom → stage   ${g(bottom?.bottom, r('[data-od-id="app"]')?.bottom)}`);
      lines.push(`right pane → stage    ${g(right?.right, r('[data-od-id="app"]')?.right)}`);
      paint(); return;
    }
    if (SUITE === 'A') await suiteA();
    else if (SUITE === 'C') await suiteC();
    else if (SUITE === 'D') await suiteD();
    else if (SUITE === 'E') await suiteE();
    else if (SUITE === 'F') await suiteF();
    else if (SUITE === 'G') await suiteG();
    else if (SUITE === 'H') await suiteH();
    else if (SUITE === 'I') await suiteI();
    else if (SUITE === 'J') await suiteJ();
    else if (SUITE === 'K') await suiteK();
    else if (SUITE === 'L') await suiteL();
    else if (SUITE === 'M') await suiteM();
    else if (SUITE === 'Q') await suiteQ();
    else if (SUITE === 'R') await suiteR();
    else if (SUITE === 'S') await suiteS();
    else if (SUITE === 'T') await suiteT();
    else if (SUITE === 'U') await suiteU();
    else if (SUITE === 'V') await suiteV();
    else if (SUITE === 'W') await suiteW();
    else if (SUITE === 'Y') await suiteY();
    else if (SUITE === 'Z') await suiteZ();
    else if (SUITE === 'AA') await suiteAA();
    else if (SUITE === 'AB') await suiteAB();
    else if (SUITE === 'AC') await suiteAC();
    else if (SUITE === 'AD') await suiteAD();
    else if (SUITE === 'AE') await suiteAE();
    else if (SUITE === 'AF') await suiteAF();
    else if (SUITE === 'AG') await suiteAG();
    else if (SUITE === 'AH') await suiteAH();
    else await suiteB();
    lines.push(''); lines.push('done'); paint();
  })();
}, 0);
