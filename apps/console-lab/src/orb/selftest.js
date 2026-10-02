/* ── SELF-TEST ── drives the orb with synthetic pointer events and measures.
   The exporter snapshots ~500ms after boot, so the suite is in short parts.
   SUITE: ring · born · assistant · drag · flick · variants · show-ring ·
          show-ring-centre · show-ring-edge · show-plate · show-plate-centre ·
          show-menu · show-panel                                              */
export const SELFTEST = false;
const SUITE = 'ring';
if (SELFTEST) setTimeout(async () => {
  const lines = [];
  const box = document.createElement('pre'); box.id = 'selftest'; box.textContent = 'self-test…';
  if (!SUITE.startsWith('show')) document.body.appendChild(box);
  window.__MOTION_MS = 40;
  document.documentElement.style.setProperty('--dur-panel', '40ms');
  const t0 = performance.now();
  const paint = () => { box.textContent = 'orb · ' + SUITE + ' · t=' + Math.round(performance.now() - t0) + 'ms\n' + lines.join('\n'); };
  setInterval(paint, 20);
  const row = (name, ok, detail) => { lines.push(name.padEnd(24) + (ok ? 'PASS  ' : 'FAIL  ') + detail); paint(); };
  const frame = () => new Promise(r => requestAnimationFrame(r));
  const wait = (n) => new Promise(r => setTimeout(r, n));
  const q = (s) => document.querySelector(s);
  const centre = (el) => { const r = el.getBoundingClientRect(); return { x: r.left + r.width / 2, y: r.top + r.height / 2 }; };
  const pe = (el, type, x, y) => el.dispatchEvent(new PointerEvent(type, { bubbles: true, cancelable: true, clientX: x, clientY: y, button: 0, buttons: 1, pointerId: 1, pointerType: 'mouse', isPrimary: true }));
  const press = async (el) => { const c = centre(el); pe(el, 'pointerdown', c.x, c.y); await frame(); pe(el, 'pointerup', c.x, c.y); await frame(); };
  const key = (el, k) => { el.dispatchEvent(new KeyboardEvent('keydown', { key: k, bubbles: true })); el.dispatchEvent(new KeyboardEvent('keyup', { key: k, bubbles: true })); };

  // The renderer resizes its viewport during boot, so measure fresh every time.
  const S = () => q('[data-od-id="stage"]').getBoundingClientRect();
  const orb = q('[data-od-id="orb"]');
  let c0 = centre(orb);

  if (SUITE === 'show-ring') { await press(orb); return; }
  const dragTo = async (x, y) => {
    const c = centre(orb);
    pe(orb, 'pointerdown', c.x, c.y); await frame(); pe(orb, 'pointermove', x, y); await wait(110); pe(orb, 'pointerup', x, y); await frame();
  };
  if (SUITE === 'show-ring-centre') { const s = S(); await dragTo(s.width / 2, s.top + s.height / 2); await press(orb); return; }
  if (SUITE === 'show-ring-edge') { const s = S(); await dragTo(s.width / 2, s.bottom); await press(orb); return; }
  if (SUITE === 'show-plate') { q('[data-od-id="lab-ring-plate"]').click(); await frame(); await press(orb); return; }
  if (SUITE === 'show-plate-centre') { q('[data-od-id="lab-ring-plate"]').click(); await frame(); const s = S(); await dragTo(s.width / 2, s.top + s.height / 2); await press(orb); return; }
  if (SUITE === 'show-menu') { q('[data-od-id="lab-opens-menu"]').click(); await frame(); await press(q('[data-od-id="orb"]')); return; }

  if (SUITE === 'born') {
    // Items are born at the orb and travel out: distance from the centre grows frame by frame.
    window.__MOTION_MS = undefined; // real motion for this one
    await press(orb);
    const first = () => q('[data-od-id="orb-item-ask"]');
    // Distance from the orb as it is now: the boot-time resize can move the orb mid-run.
    const dist = () => { const c = centre(orb); const r = first().getBoundingClientRect(); return Math.round(Math.hypot(r.left + r.width / 2 - c.x, r.top + r.height / 2 - c.y)); };
    const path = [];
    for (let i = 0; i < 12; i++) { await frame(); path.push(dist()); }
    const ringR = () => Math.round(parseFloat(getComputedStyle(q('[data-od-id="orb-radial"]')).getPropertyValue('--radius')));
    row('starts at the orb', path[0] < ringR() * 0.5, `first ${path[0]} of ${ringR()}`);
    row('travels outward', path.slice(0, 8).every((d, i, a) => i === 0 || d >= a[i - 1]), path.slice(0, 8).join(','));
    // Overshoots a touch, then settles: wait for two frames at rest on the ring.
    let settled = 0, lastD = 0;
    for (let i = 0; i < 40 && settled < 2; i++) { await frame(); lastD = dist(); settled = Math.abs(lastD - ringR()) <= 2 ? settled + 1 : 0; }
    row('arrives at the ring', settled >= 2, `ends ${lastD} of ${ringR()}`);
    const op = parseFloat(getComputedStyle(first()).opacity);
    row('fully visible at rest', op === 1, `opacity ${op}`);
    q('[data-od-id="lab-ring-plate"]').click(); await frame(); await frame();
    const plate = q('[data-od-id="orb-plate"]');
    const pb = plate?.getBBox();
    row('plate under the ring', !!plate && pb && pb.width > ringR(), plate ? `plate ${Math.round(pb.width)} wide` : 'no plate');
    const orbZ = getComputedStyle(orb).zIndex, wrapZ = getComputedStyle(q('.od-orb-ringwrap')).zIndex;
    row('orb above the plate', Number(orbZ) > Number(wrapZ), `orb ${orbZ} · ring ${wrapZ}`);
  }

  if (SUITE === 'assistant') {
    await press(orb); await wait(60);
    q('[data-od-id="orb-item-ask"]').dispatchEvent(new MouseEvent('click', { bubbles: true })); await frame(); await wait(40);
    row('ask opens assistant', !!q('[data-od-id="chat-input"]') && !q('[data-od-id="orb-radial"]'), q('[data-od-id="lab-last"]').textContent);
    row('ask mode pressed', q('[data-od-id="assistant-mode-ask"]')?.getAttribute('aria-pressed') === 'true', '');
    q('[data-od-id="assistant-mode-voice"]').click(); await frame();
    row('switches to voice', !!q('.voice') && !q('[data-od-id="chat-input"]'), '');
    q('[data-od-id="assistant-mode-ask"]').click(); await frame();
    row('and back to ask', !!q('[data-od-id="chat-input"]'), '');
    q('[data-od-id="orb-panel-close"]').click(); await frame();
    row('close', orb.getAttribute('aria-expanded') === 'false' && !q('[data-od-id="orb-radial"]'), '');
    await wait(60);
    await press(orb); await wait(60);
    q('[data-od-id="orb-item-voice"]').dispatchEvent(new MouseEvent('click', { bubbles: true })); await frame(); await wait(40);
    row('voice opens listening', !!q('.voice'), q('[data-od-id="lab-last"]').textContent);
    const pr = q('.od-orb-panel')?.getBoundingClientRect(), s = S();
    row('panel inside stage', pr && pr.left >= s.left && pr.right <= s.right && pr.bottom <= s.bottom, pr ? `${Math.round(pr.left)}–${Math.round(pr.right)}` : 'no panel');
  }
  if (SUITE === 'show-panel') { q('[data-od-id="lab-opens-panel"]').click(); await frame(); await press(q('[data-od-id="orb"]')); return; }

  if (SUITE === 'ring') {
    row('starts bottom-right', c0.x > S().right - 60 && c0.y > S().bottom - 60, `centre ${Math.round(c0.x)},${Math.round(c0.y)}`);
    await press(orb); await wait(200); // the items radiate out over a few staggered frames
    c0 = centre(orb);
    const ring = q('[data-od-id="orb-radial"]');
    const items = [...document.querySelectorAll('.od-orb-item')];
    const radii = items.map(i => Math.round(Math.hypot(centre(i).x - c0.x, centre(i).y - c0.y)));
    row('press opens ring', !!ring && items.length === 5, `${items.length} items · radii ${radii.join(',')}`);
    const ringR = Math.round(parseFloat(getComputedStyle(ring).getPropertyValue('--radius')));
    row('items on the circle', radii.every(r => Math.abs(r - ringR) <= 2), `ring radius ${ringR}`);
    const inStage = items.every(i => { const r = i.getBoundingClientRect(), s = S(); return r.left >= s.left && r.right <= s.right && r.top >= s.top && r.bottom <= s.bottom; });
    row('ring fits the corner', inStage, '');
    row('ring is a menu', ring?.getAttribute('role') === 'menu' && items.every(i => i.getAttribute('role') === 'menuitem'), '');
    row('first item focused', document.activeElement === items[0], String(document.activeElement?.textContent));
    key(document.activeElement, 'ArrowDown'); await frame();
    row('arrow moves focus', document.activeElement === items[1], String(document.activeElement?.textContent));
    key(document.activeElement, 'ArrowRight'); await frame();
    key(document.activeElement, 'ArrowLeft'); await frame(); key(document.activeElement, 'ArrowLeft'); await frame();
    row('ring walks both ways', document.activeElement === items[0], String(document.activeElement?.textContent));
    const go = q('[data-od-id="orb-item-go"]');
    go.dispatchEvent(new MouseEvent('click', { bubbles: true })); await frame(); await wait(30);
    const back = q('[data-od-id="orb-item-__back"]');
    row('submenu level', !!back && q('[data-od-id="orb-radial"]')?.dataset.level === '1', `${document.querySelectorAll('.od-orb-item').length} items on level 1`);
    back.dispatchEvent(new MouseEvent('click', { bubbles: true })); await frame();
    row('back returns', q('[data-od-id="orb-radial"]')?.dataset.level === '0' && document.querySelectorAll('.od-orb-item').length === 5, '');
    go.isConnected || (await frame());
    q('[data-od-id="orb-item-go"]').dispatchEvent(new MouseEvent('click', { bubbles: true })); await frame();
    q('[data-od-id="orb-item-go-runway"]').dispatchEvent(new MouseEvent('click', { bubbles: true })); await frame(); await wait(30);
    row('leaf acts and closes', q('[data-od-id="lab-last"]').textContent === 'go-runway' && !q('[data-od-id="orb-radial"]'), q('[data-od-id="lab-last"]').textContent);
    row('focus back on orb', document.activeElement === orb, String(document.activeElement?.dataset.odId));
    await press(orb); await wait(30);
    key(document.activeElement, 'Escape'); await frame();
    row('escape closes', !q('[data-od-id="orb-radial"]') && document.activeElement === orb, '');
    const before = centre(orb);
    key(orb, 'ArrowLeft'); await frame();
    row('arrow nudges orb', Math.round(centre(orb).x) === Math.round(before.x) - 16, `${Math.round(centre(orb).x)} from ${Math.round(before.x)}`);
  }

  if (SUITE === 'drag') {
    await frame(); c0 = centre(orb);
    pe(orb, 'pointerdown', c0.x, c0.y); await frame();
    pe(orb, 'pointermove', c0.x - 200, c0.y - 100); await frame();
    pe(orb, 'pointermove', c0.x - 400, c0.y - 200); await frame();
    row('drag flag', orb.dataset.dragging === 'true' && orb.dataset.pressed === 'true', `dragging=${orb.dataset.dragging} pressed=${orb.dataset.pressed}`);
    await wait(110); // a pause before release is a drop, not a flick
    pe(orb, 'pointerup', c0.x - 400, c0.y - 200); await frame(); await frame();
    const c1 = centre(orb);
    // The boot-time viewport resize may clamp the target; allow for it.
    const ex = Math.min(S().right - 38, Math.max(S().left + 38, c0.x - 400)), ey = Math.min(S().bottom - 38, Math.max(S().top + 38, c0.y - 200));
    row('drag moves it', Math.abs(c1.x - ex) < 2 && Math.abs(c1.y - ey) < 2 && !q('[data-od-id="orb-radial"]'), `now ${Math.round(c1.x)},${Math.round(c1.y)} expected ${Math.round(ex)},${Math.round(ey)}`);
    row('release clears flags', orb.dataset.dragging === 'false' && orb.dataset.pressed === 'false', '');
    row('a paused drop stays', Math.round(centre(orb).x) === Math.round(c1.x), `${Math.round(centre(orb).x)} vs ${Math.round(c1.x)}`);
    pe(orb, 'pointerdown', c1.x, c1.y); await frame(); pe(orb, 'pointermove', c1.x - 3000, c1.y); await wait(110); pe(orb, 'pointerup', c1.x - 3000, c1.y); await frame();
    const c2 = centre(orb);
    row('clamped to stage', c2.x >= S().left + 26 && c2.x <= S().left + 40, `x ${Math.round(c2.x)} vs stage.left ${Math.round(S().left)}`);
  }

  if (SUITE === 'flick') {
    // Flick at the right wall: it keeps going after release, hits, comes back, stays inside.
    await frame(); await dragTo(S().left + 200, centre(orb).y);
    const start = centre(orb);
    const stage = S();
    pe(orb, 'pointerdown', start.x, start.y);
    const dx = stage.width * 0.55;
    for (let i = 1; i <= 6; i++) { pe(orb, 'pointermove', start.x + dx * i / 6, start.y); await wait(10); }
    pe(orb, 'pointerup', start.x + dx, start.y);
    const release = centre(orb).x;
    const xs = [];
    for (let i = 0; i < 22; i++) { await frame(); xs.push(Math.round(centre(orb).x)); }
    const maxX = Math.max(...xs);
    row('flick keeps moving', maxX > release + 30, `release ${Math.round(release)} → max ${maxX}`);
    // Frames sample the path, so the wall itself may fall between two samples.
    row('bounces off wall', maxX >= stage.right - 70 && xs[xs.length - 1] < maxX - 5, `wall ${Math.round(stage.right)} · max ${maxX} · now ${xs[xs.length - 1]}`);
    row('never leaves stage', xs.every(x => x >= stage.left && x <= stage.right), '');
    row('no menu while moving', !q('[data-od-id="orb-radial"]'), '');
  }

  if (SUITE === 'variants') {
    q('[data-od-id="lab-opens-menu"]').click(); await frame();
    const o2 = q('[data-od-id="orb"]');
    await press(o2); await wait(30);
    const menu = q('[data-od-id="orb-menu"]');
    const rows = menu ? menu.querySelectorAll('[role="menuitem"]').length : 0;
    row('menu variant', !!menu && menu.getAttribute('role') === 'menu' && rows === 8, `${rows} rows`);
    row('menu focused', menu?.contains(document.activeElement), String(document.activeElement?.textContent));
    key(document.activeElement, 'Escape'); await frame(); await wait(30);
    row('menu escape', !q('[data-od-id="orb-menu"]'), '');
    q('[data-od-id="lab-opens-panel"]').click(); await frame();
    const o3 = q('[data-od-id="orb"]');
    await press(o3); await wait(30);
    const panelEl = q('.od-orb-panel');
    // The panel follows the orb, which the boot-time resize may still be clamping; allow a few frames.
    let pr, stage, fits = false;
    for (let i = 0; i < 10 && !fits; i++) {
      await frame(); pr = panelEl?.getBoundingClientRect(); stage = S();
      fits = pr && pr.left >= stage.left && pr.right <= stage.right && pr.bottom <= stage.bottom;
    }
    row('panel variant', !!q('[data-od-id="chat-input"]') && fits, pr ? `panel ${Math.round(pr.left)}–${Math.round(pr.right)} × ${Math.round(pr.top)}–${Math.round(pr.bottom)} in ${Math.round(stage.right)}×${Math.round(stage.bottom)}` : 'no panel');
    const orbZ = getComputedStyle(q('[data-od-id="orb-layer"]')).zIndex, panZ = getComputedStyle(panelEl.closest('.od-overlay-float')).zIndex;
    row('stacking', Number(orbZ) > Number(panZ) && Number(panZ) > 0, `orb ${orbZ} · panel ${panZ}`);
    q('[data-od-id="orb-panel-close"]').click(); await frame();
    row('panel close', o3.getAttribute('aria-expanded') === 'false', `expanded=${o3.getAttribute('aria-expanded')}`);
    await wait(60);
    await press(q('[data-od-id="orb"]')); await wait(30);
    q('[data-od-id="assistant-mode-voice"]').click(); await frame();
    row('voice mode in panel', !!q('.voice') && !q('[data-od-id="chat-input"]'), '');
  }
}, 40);
