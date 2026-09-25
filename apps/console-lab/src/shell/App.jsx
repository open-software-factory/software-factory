import React, { useState, useRef, useEffect, useLayoutEffect, useCallback, useMemo } from 'react';
import {
  FLOAT_KEY, NAV_ORDER, NAV_WIDTH, NO_INSPECTION, NO_INTERRUPT, ORB_KEY,
  RAIL_COMPACT, RAIL_GAP, SETTINGS_KEY, SLOT_LABEL, STORAGE_KEY,
  SURFACES as SURFACE_REGISTRY,
  acceptedSlots, bottomHeight, breakpointFor, buildCommands, clamp, clampFloat,
  createStorage, fitToViewport, fromPreset,
  inspectMode as computeInspectMode, inspectPlan,
  interruptReduce, isPanelOpen, loadFloat, loadSettings, memoryBackend,
  nextTheme, noPopOut, orbIntentOf, orbMenu, placementOf, popOutLabel,
  railWidthFor, reduceWorkspace, saveFloat, saveSettings, saveWorkspace,
  sectionOf, showsBanner,
  sideWidth as computeSideWidth,
} from '@open-software-factory/console-model';
import { Overlay } from '@open-software-factory/console-ui/overlay';
import { CommandPalette, usePaletteShortcut } from '@open-software-factory/console-ui/palette';
import { Popover, ChoiceMenu } from '@open-software-factory/console-ui/popover';
import { FloatingOrb } from '@open-software-factory/console-ui/orb';
import { DATA } from '../data/index.js';

const SC = { ok: 'var(--state-ok)', active: 'var(--state-active)', blocked: 'var(--state-blocked)', fail: 'var(--state-fail)', idle: 'var(--state-idle)' };
const EASE = 'cubic-bezier(0.2, 0, 0.2, 1)';

/*
  A selection that moves. One highlight element per strip or list sits behind
  the buttons and glides to whichever is current, so a change of view or tab
  reads as the same chip travelling rather than one light going off and
  another coming on. Measured after layout; CSS transitions the box.
*/
function useSlidingHighlight(ref, selector, key) {
  const [box, setBox] = useState(null);
  useLayoutEffect(() => {
    const root = ref.current;
    const el = root?.querySelector(selector);
    if (!root || !el) { setBox(null); return; }
    const r = el.getBoundingClientRect(), b = root.getBoundingClientRect();
    setBox({ left: r.left - b.left + root.scrollLeft, top: r.top - b.top + root.scrollTop, width: r.width, height: r.height });
  }, [key]);
  return box;
}
/* WAAPI rather than a CSS transition, for the same reason pane sizes use it:
   React writes the final box; this interpolates from the previous one. */
function SlideHighlight({ box, id }) {
  const ref = useRef(null);
  const prev = useRef(null);
  useLayoutEffect(() => {
    const el = ref.current, from = prev.current;
    prev.current = box;
    if (!el || !box || !from) return;
    if (['left', 'top', 'width', 'height'].every(k => from[k] === box[k])) return;
    const px = (b) => ({ left: b.left + 'px', top: b.top + 'px', width: b.width + 'px', height: b.height + 'px' });
    el.animate([px(from), px(box)], { duration: window.__MOTION_MS || 200, easing: EASE });
  }, [box]);
  return box ? <div ref={ref} className="slide-hl" data-od-id={id} style={box} aria-hidden="true" /> : null;
}

/*
  Animate a size change with WAAPI rather than a CSS transition. React writes
  the final value to the inline style; this interpolates from the previous one.
  Skipped while a pointer drag is in flight so resizing stays 1:1.
*/
function useAnimatedSize(ref, value, prop, enabled, dragging) {
  const prev = React.useRef(value);
  React.useLayoutEffect(() => {
    const from = prev.current;
    prev.current = value;
    const el = ref.current;
    if (!el || !enabled || dragging.current || from === value) return;
    el.animate([{ [prop]: from + 'px' }, { [prop]: value + 'px' }],
      { duration: window.__MOTION_MS || 280, easing: EASE });
  }, [value]);
}
/*
  The design target binds the memory backend. The browser and Tauri adapters
  live outside this bundle on purpose — see packages/console-model/src/storage.ts.
  Layout therefore does not survive a reload here, which is fine: persistence
  is covered by tests, and the artifact stays free of Web Storage so the
  preview can serve it from a URL rather than a sandbox.
*/
const store = createStorage(memoryBackend(), [STORAGE_KEY, SETTINGS_KEY, FLOAT_KEY, ORB_KEY]);

/* ── content ────────────────────────────────────────────────────────────── */
const Head = ({ t, tag }) => <div className="fx-head"><h3>{t}</h3>{tag && <span className="fx-tag">{tag}</span>}</div>;

/* Content selection: click, Enter or Space picks an item for the inspector. */
const selectable = (api, ref) => ({
  role: 'button', tabIndex: 0, 'data-item': ref, 'data-selected': String(api.inspected?.id === ref),
  onClick: () => api.select(ref),
  onKeyDown: (e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); api.select(ref); } },
});

function Floor({ api }) {
  const d = DATA.floor, by = Object.fromEntries(d.nodes.map(n => [n.id, n]));
  return (
    <div className="canvas">
      <Head t={d.title} tag={d.tag} />
      <svg viewBox={d.viewBox} style={{ width: '100%', maxHeight: '21.25rem' }} role="img" aria-label="Work topology">
        {d.edges.map((e, i) => {
          const a = by[e.from], b = by[e.to];
          const x1 = a.x + a.w, y1 = a.y + a.h / 2, x2 = b.x, y2 = b.y + b.h / 2, m = (x1 + x2) / 2;
          const p = b.state === 'idle' || a.state === 'blocked';
          return <path key={i} className={'edge' + (p ? ' pending' : '')} d={`M${x1} ${y1} C${m} ${y1}, ${m} ${y2}, ${x2} ${y2}`} />;
        })}
        {d.nodes.map(n => (
          <g key={n.id} {...selectable(api, n.sub.split(' · ')[0])} style={{ cursor: 'pointer' }}>
            <rect x={n.x} y={n.y} width={n.w} height={n.h} rx="4" fill="var(--surface-2)"
              stroke={api.inspected?.id === n.sub.split(' · ')[0] ? 'var(--accent)' : 'var(--border-subtle)'} />
            <circle cx={n.x + 13} cy={n.y + n.h / 2} r="3.2" fill={SC[n.state]} />
            <text x={n.x + 26} y={n.y + 19} fill="var(--fg)" fontSize="11">{n.title}</text>
            <text x={n.x + 26} y={n.y + 32} fill="var(--fg-faint)" fontSize="9" fontFamily="var(--font-mono)">{n.sub}</text>
          </g>
        ))}
      </svg>
      <div className="legend">{d.legend.map(l => <span key={l.state}><i className="dot" style={{ background: SC[l.state] }} />{l.label}</span>)}</div>
    </div>
  );
}

const Board = ({ api }) => (
  <div className="pad"><Head t={DATA.board.title} tag={DATA.board.tag} />
    <div className="board">{DATA.board.columns.map(c => (
      <div key={c.name}>
        <div className="colhead"><span>{c.name}</span><span>{c.items.length}</span></div>
        {c.items.map(i => (
          <div key={i.id} className="card" {...selectable(api, i.id)}>
            <div className="fxm ref" style={{ color: 'var(--fg-faint)' }}><i className="dot" style={{ background: SC[i.state] }} />{i.id}</div>
            <div className="ttl">{i.title}</div>
            <div className="fxm" style={{ color: 'var(--fg-muted)', marginTop: 5 }}>{i.who}</div>
          </div>
        ))}
      </div>
    ))}</div>
  </div>
);

const Runway = () => (
  <div className="pad"><Head t={DATA.runway.title} tag={DATA.runway.tag} />
    <table className="tbl">
      <thead><tr>{DATA.runway.columns.map(c => <th key={c}>{c}</th>)}</tr></thead>
      <tbody>{DATA.runway.rows.map(r => (
        <tr key={r.release}>
          <td className="k fxm">{r.release}</td><td>{r.scope}</td>
          <td className="k"><i className="dot" style={{ background: SC[r.state], marginRight: 7 }} />{r.stateLabel}</td>
          <td className="fxm">{r.gates}</td><td className="fxm">{r.when}</td>
        </tr>
      ))}</tbody>
    </table>
  </div>
);

/* Two vocabularies, never the same marker:
   view  — a centre destination. Exactly one is current; others may be open as
           background tabs. Radio semantics, accent bar on the current one.
   panel — a surface beside the centre. Any number open at once. Switch
           semantics, a small open/closed indicator, never the current marker. */
/* An item that changes section must not teleport: FLIP it from where it was. */
function useNavReflow(rootRef, signature, enabled) {
  const prev = React.useRef(new Map());
  React.useLayoutEffect(() => {
    const root = rootRef.current;
    if (!root) return;
    const now = new Map();
    root.querySelectorAll('[data-nav-item]').forEach(el => now.set(el.dataset.navItem, el.getBoundingClientRect()));
    if (enabled) {
      now.forEach((box, id) => {
        const was = prev.current.get(id);
        if (!was) return;
        const dy = was.top - box.top;
        if (Math.abs(dy) < 1) return;
        root.querySelector(`[data-nav-item="${id}"]`)
          .animate([{ transform: `translateY(${dy}px)` }, { transform: 'none' }],
            { duration: window.__MOTION_MS || 280, easing: EASE });
      });
    }
    prev.current = now;
  }, [signature]);
}

function Nav({ api }) {
  const rootRef = React.useRef(null);
  const primary = NAV_ITEMS.filter(n => api.sectionOf(n.id) === 'primary');
  const panels = NAV_ITEMS.filter(n => api.sectionOf(n.id) === 'panel');
  useNavReflow(rootRef, primary.map(n => n.id).join() + '|' + panels.map(n => n.id).join(), api.anim);
  const navHl = useSlidingHighlight(rootRef, '.nav button[aria-current="page"]',
    `${api.centreActive}|${primary.map(n => n.id).join()}`);

  /* A view is where you are. Radio: exactly one current, accent marker. */
  const View = (n) => {
    const current = api.centreActive === n.id;
    return (
      <li key={n.id} data-nav-item={n.id}><button type="button" data-kind="view" data-nav={n.id}
        data-open={String(api.isCentreOpen(n.id))} aria-current={current ? 'page' : 'false'}
        onClick={() => api.openCentre(n.id)}>
        <span className="mk" aria-hidden="true" /><span>{n.label}</span><span className="c">{n.count}</span>
      </button></li>
    );
  };
  /* A panel is something you have open beside it. Switch: any number, no marker. */
  const Panel = (n) => {
    const open = api.isPanelOpen(n.id);
    return (
      <li key={n.id} data-nav-item={n.id}><button type="button" data-kind="panel" data-nav={n.id} role="switch"
        aria-checked={String(open)} onClick={() => api.toggleSurface(n.id)}
        title={(open ? 'Close ' : 'Open ') + n.label}>
        <span className="sw" aria-hidden="true" /><span>{n.label}</span><span className="c">{n.count}</span>
      </button></li>
    );
  };

  return (
    <div className="pad" ref={rootRef} data-slides="true">
      <SlideHighlight box={navHl} id="nav-highlight" />
      <ul className="nav">
        {NAV_GROUP_TITLES && <li className="g">Views</li>}
        {primary.map(View)}
        {primary.length === 0 && <li className="g-empty">Nothing in the centre</li>}
      </ul>
      {/* The rule carries the split on its own; the words were restating it. */}
      <hr className="navsep" />
      <ul className="nav" data-section="secondary">
        {NAV_GROUP_TITLES && <li className="g">Panels</li>}
        {panels.map(Panel)}
        {panels.length === 0 && <li className="g-empty">All panels are in the centre</li>}
      </ul>
    </div>
  );
}

const Attention = ({ api }) => (
  <div className="pad">
    <div className="fxm" style={{ color: 'var(--fg-faint)', textTransform: 'uppercase', paddingBottom: 12 }}>{DATA.attention.summary}</div>
    {DATA.attention.items.map(i => (
      <div key={i.id} className={'att' + (i.needsYou ? ' you' : '')} {...selectable(api, i.id)}>
        <i className="dot" style={{ background: SC[i.state], marginTop: 5 }} />
        <div style={{ minWidth: 0 }}>
          <div className="hd">{i.headline}</div><div className="dt">{i.detail}</div>
          <div className="mt"><span className="age">{i.age}</span><span className="verb">{i.verb} →</span></div>
        </div>
      </div>
    ))}
  </div>
);

const Inspector = ({ api }) => {
  const picked = useInspected();
  /* A board row picked from the palette: the header carries its reference,
     the body what the board knows. Evidence and actions arrive with the real
     inspector; nothing is invented to fill the gap. */
  if (picked) return (
    <div className="pad" data-od-id="inspector-picked">
      <Crumbs path={picked.path} current={picked.id} api={api} />
      <h3 style={{ fontSize: 'var(--t-section)', margin: '0 0 6px', color: 'var(--fg-strong)', fontWeight: 600, lineHeight: 1.35 }}>{picked.title}</h3>
      <dl className="fields">
        {picked.fields.map(([k, v]) => <React.Fragment key={k}><dt>{k}</dt><dd>{v}</dd></React.Fragment>)}
        <dt>state</dt><dd style={{ color: SC[picked.state] || 'var(--fg)' }}>{picked.state}</dd>
      </dl>
    </div>
  );
  return (
  <div className="pad">
    {/* The header carries the reference now, so the body carries the headline
        alone rather than repeating "service-auth#5 ·" two lines apart. */}
    <h3 style={{ fontSize: 'var(--t-section)', margin: '0 0 6px', color: 'var(--fg-strong)', fontWeight: 600, lineHeight: 1.35 }}>
      {(DATA.inspector.title || '').split(' · ').slice(1).join(' · ') || DATA.inspector.title}
    </h3>
    <p style={{ color: 'var(--fg-muted)', lineHeight: 1.5, margin: 0 }}>{DATA.inspector.summary}</p>
    <dl className="fields">{DATA.inspector.fields.map(f => <React.Fragment key={f.label}><dt>{f.label}</dt><dd>{f.value}</dd></React.Fragment>)}</dl>
    {DATA.inspector.evidence.map(e => <div key={e.id} className="ev"><span className="fxm" style={{ color: 'var(--fg-faint)' }}>{e.id}</span><span>{e.text}</span></div>)}
    <div style={{ display: 'flex', gap: 8, marginTop: 16 }}><button className="btn pri">Approve</button><button className="btn">Request changes</button></div>
  </div>
  );
};

const Logs = () => (
  <div className="pad"><Head t={DATA.logs.title} tag={DATA.logs.run} />
    <div className="log">{DATA.logs.lines.map((l, i) => <div key={i}><span className="t">{l.t}</span><span className={l.level}>{l.text}</span></div>)}</div>
  </div>
);

const Recorder = () => (
  <div className="pad">
    <div className="tl-track">
      {DATA.timeline.events.map((e, i) => <span key={i} className="tick" title={e.label} style={{ left: e.at + '%', background: SC[e.state] }} />)}
      <span className="play" style={{ left: DATA.timeline.playheadPercent + '%' }} />
    </div>
    <div className="tl-scale">{DATA.timeline.scale.map(s => <span key={s}>{s}</span>)}</div>
  </div>
);

/*
  Placement metadata — width appetite, rail tolerance, home slot, accepted
  slots — lives in shell/src/surfaces.ts and is compiled in above. This layer
  only binds an id to the component that draws it, so the tested model has no
  idea React exists.
*/
const BODIES = { floor: Floor, board: Board, runway: Runway, attention: Attention, inspector: Inspector, logs: Logs, recorder: Recorder };
const BARE = new Set(['floor']);
const SURFACES = Object.fromEntries(
  Object.entries(SURFACE_REGISTRY).map(([id, s]) => [id, { ...s, Body: BODIES[id], bare: BARE.has(id) }])
);
/*
  What a panel is *showing*, not what the component is called. "Inspector" is
  the name of a mechanism; an operator needs the kind of thing and which one.
  So a panel may declare a subject — a type and a reference — and the header
  shows that instead of the surface's own label.

  Surfaces that hold a collection rather than one thing (the attention queue,
  the recorder) have no single subject and fall back to their label.
*/
/* Group headings in the nav. Off: the separator already says where the split
   is, and two words of chrome for six items is a poor trade. Flip to true to
   compare. */
const NAV_GROUP_TITLES = false;

/* What the inspector is showing. The palette sets it today; content selection
   (a node, a card) will set it the same way. Null means the fixture item. */
const InspectedContext = React.createContext(null);
const useInspected = () => React.useContext(InspectedContext);
const subjectFor = (id, inspected) => {
  if (id === 'inspector') {
    const ref = inspected ? inspected.id : (DATA.inspector.title || '').split(' · ')[0];
    return ref ? { kind: 'Work item', ref } : null;
  }
  if (id === 'logs') {
    const ref = (DATA.logs.run || '').split(' · ')[0];
    return ref ? { kind: 'Run output', ref } : null;
  }
  return null;
};

/* Counts stay in the fixture; the nav order and sections come from the model. */
const NAV_COUNT = Object.fromEntries(DATA.nav.groups.flatMap(g => g.items.map(n => [n.id, n.count])));
const NAV_ITEMS = NAV_ORDER.map(id => ({ id, label: SURFACES[id].label, count: NAV_COUNT[id] ?? '' }));

/* The palette searches the board's rows and the run behind the log fixture.
   Evidence has no fixture yet, so it is honestly absent rather than invented. */
const WORK_ITEMS = DATA.board.columns.flatMap(c => c.items.map(i =>
  ({ id: i.id, title: i.title, meta: `${i.who} ${c.name}`, column: c.name, who: i.who, state: i.state })));
const RUNS = DATA.logs.run ? [{ id: DATA.logs.run.split(' · ')[0], title: DATA.logs.run, surface: 'logs' }] : [];

/* One shape for anything the inspector can show, whichever list it came from.
   Only what that list knows; evidence and actions arrive with the real data. */
const lookupItem = (ref) => {
  if (!ref) return null;
  const w = WORK_ITEMS.find(w => w.id === ref);
  if (w) return { id: w.id, title: w.title, state: w.state, fields: [['labels', w.who]],
    path: [{ label: 'Board', view: 'board' }, { label: w.column, view: 'board' }] };
  const a = DATA.attention.items.find(i => i.id === ref);
  if (a) return { id: a.id, title: a.headline, state: a.state, fields: [['detail', a.detail]],
    path: [{ label: 'Attention', panel: 'attention' }, { label: a.age, panel: 'attention' }] };
  const n = DATA.floor.nodes.find(n => n.sub.split(' · ')[0] === ref);
  if (n) return { id: ref, title: n.title, state: n.state, fields: [['priority', n.sub.split(' · ')[1] ?? '—']],
    path: [{ label: 'Floor', view: 'floor' }] };
  return { id: ref, title: ref, state: 'idle', fields: [], path: [] };
};

/* Breadcrumb (§5.5.2): where the inspected item came from, each level a jump
   back in one action. The item itself is the current level and not a link. */
function Crumbs({ path, current, api }) {
  if (!path?.length) return null;
  const go = (level) => level.view ? api.openCentre(level.view) : api.openPanel(level.panel);
  return (
    <nav className="crumbs" aria-label="Where this came from" data-od-id="crumbs">
      <ol>
        {path.map((level, i) => (
          <li key={i}><button type="button" className="crumb" data-crumb={level.label} onClick={() => go(level)}>{level.label}</button></li>
        ))}
        <li aria-current="location"><span className="crumb current">{current}</span></li>
      </ol>
    </nav>
  );
}

const I = {
  close: <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round"><path d="M4 4l8 8M12 4L4 12" /></svg>,
  rail: <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4"><rect x="2" y="3" width="12" height="10" rx="1" /><path d="M6 3v10" /></svg>,
  float: <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinejoin="round"><rect x="2" y="4.5" width="8" height="7.5" rx="1" /><path d="M6 4.5V3.2A1.2 1.2 0 0 1 7.2 2H13a1 1 0 0 1 1 1v5.8A1.2 1.2 0 0 1 12.8 10H11.5" /></svg>,
  menu: <svg width="15" height="15" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round"><path d="M2.5 4h11M2.5 8h11M2.5 12h11" /></svg>,
  sun: <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round"><circle cx="8" cy="8" r="3.1" /><path d="M8 1.4v1.5M8 13.1v1.5M14.6 8h-1.5M2.9 8H1.4M12.66 3.34l-1.06 1.06M4.4 11.6l-1.06 1.06M12.66 12.66l-1.06-1.06M4.4 4.4L3.34 3.34" /></svg>,
  moon: <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinejoin="round"><path d="M13.2 9.6A5.6 5.6 0 0 1 6.4 2.8a5.6 5.6 0 1 0 6.8 6.8Z" /></svg>,
  focus: <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.35" strokeLinecap="round" strokeLinejoin="round"><path d="M2 5.6V3a1 1 0 0 1 1-1h2.6M14 5.6V3a1 1 0 0 0-1-1h-2.6M2 10.4V13a1 1 0 0 0 1 1h2.6M14 10.4V13a1 1 0 0 1-1 1h-2.6" /></svg>,
  unfocus: <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.35" strokeLinecap="round" strokeLinejoin="round"><path d="M6 2.4V5a1 1 0 0 1-1 1H2.4M10 2.4V5a1 1 0 0 0 1 1h2.6M6 13.6V11a1 1 0 0 0-1-1H2.4M10 13.6V11a1 1 0 0 1 1-1h2.6" /></svg>,
  /* A panel edge with an arrow leaving it — the standard collapse/expand mark. */
  expandNav: <svg width="15" height="15" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.35" strokeLinecap="round" strokeLinejoin="round"><path d="M3 2.6v10.8" /><path d="M6.4 8h6.4M10.2 5.4L12.8 8l-2.6 2.6" /></svg>,
  /* A pushpin: pin docks the floating inspector; the struck one lifts it out. */
  pin: <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.35" strokeLinecap="round" strokeLinejoin="round"><path d="M5.5 2.5h5M6.5 2.5v3.6L4.5 8.4v1.6h7V8.4L9.5 6.1V2.5M8 10v3.6" /></svg>,
  /* Window chrome verbs for the P0 strip and dialog. */
  minimise: <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.35" strokeLinecap="round"><path d="M3.5 11.5h9" /></svg>,
  maximise: <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.35" strokeLinecap="round" strokeLinejoin="round"><rect x="3" y="3.5" width="10" height="9" rx="1" /><path d="M3 6.5h10" /></svg>,
  /* A window leaving the frame: pop-out. */
  popout: <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.35" strokeLinecap="round" strokeLinejoin="round"><path d="M7 3H3.5a1 1 0 0 0-1 1v8a1 1 0 0 0 1 1h8a1 1 0 0 0 1-1V9" /><path d="M9.5 2.5h4v4M13.5 2.5 8 8" /></svg>,
  unpin: <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.35" strokeLinecap="round" strokeLinejoin="round"><path d="M5.5 2.5h5M6.5 2.5v3.6L4.5 8.4v1.6h7V8.4L9.5 6.1V2.5M8 10v3.6" /><path d="M2.5 2.5l11 11" /></svg>,
  collapseNav: <svg width="15" height="15" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.35" strokeLinecap="round" strokeLinejoin="round"><path d="M13 2.6v10.8" /><path d="M9.6 8H3.2M5.8 5.4L3.2 8l2.6 2.6" /></svg>,
};

/*
  A pane header names what is in the pane. Where the panel holds one thing, the
  type comes first and the reference carries the weight — "Work item
  service-auth#5" tells an operator which item they are looking at, where
  "Inspector" only tells them which widget is doing the looking.
*/
function PaneTitle({ id }) {
  const subject = subjectFor(id, useInspected());
  if (!subject) return <span className="pane-title">{SURFACES[id].label}</span>;
  return (
    <span className="pane-title" data-subject={id}>
      <span className="kind">{subject.kind}</span>
      <span className="ref">{subject.ref}</span>
    </span>
  );
}

/* ── pane ───────────────────────────────────────────────────────────────── */
/* `closable` defaults off for the centre: a view is left through the nav or
   its tab, not by closing the whole centre. Side and bottom panes keep it. */
function Pane({ slot, state, api, closable = slot !== 'centre' }) {
  const bodyRef = useRef(null);
  const first = useRef(true);
  const inspected = useInspected();
  const subjectOf = (id) => subjectFor(id, inspected);
  const ids = state.panels;
  const activeId = state.active || ids[0];
  const tabsRef = useRef(null);
  const tabHl = useSlidingHighlight(tabsRef, '.tab[aria-selected="true"]', `${activeId}|${ids.join()}`);

  // Animate on tab change only — an inline ref callback would restart every render.
  useEffect(() => {
    if (first.current) { first.current = false; return; }
    if (!bodyRef.current || !api.anim) return;
    bodyRef.current.animate(
      [{ opacity: 0, transform: 'translateX(8px)' }, { opacity: 1, transform: 'none' }],
      { duration: 200, easing: 'cubic-bezier(0.2,0,0.2,1)' }
    );
  }, [activeId]);

  /* An empty centre is a real state once surfaces can be moved out of it. */
  if (!ids.length) {
    if (slot !== 'centre') return null;
    return (
      <div className="paneshell">
        <div className="pane" data-slot="centre" data-empty="true">
          <div className="empty" data-od-id="centre-empty">Nothing in the centre. Pick a view from the nav.</div>
        </div>
      </div>
    );
  }
  const S = SURFACES[activeId];
  const multi = ids.length > 1;

  /* One action. There used to be a "collapse to rail" button beside this,
     which produced exactly the same picture — the panel gone, the rail left —
     while leaving the panel open in state, so the nav went on reporting it as
     open. Closing is the only thing that means closed. */
  const actions = (
    <div className="pane-actions">
      {/* A docked inspector with a subject can float again; that is a different
          outcome from closing, so it earns a second control. */}
      {activeId === 'inspector' && api.canFloat && (
        <button className="ibtn" data-act="unpin" onClick={api.unpin}
          title={`Float ${subjectOf(activeId)?.ref ?? S.label}`}>{I.unpin}</button>
      )}
      {/* Pop-out goes through a port; without a compliant host the control
          stays visible but disabled, like a refused slot (§5.5.7). */}
      {slot !== 'centre' && (
        <button className="ibtn" data-act="popout" disabled={!api.popOut.available}
          title={popOutLabel(api.popOut, subjectOf(activeId)?.ref ?? S.label)}
          onClick={() => api.popOut.open(activeId, subjectOf(activeId)?.ref ?? S.label)}>{I.popout}</button>
      )}
      {/* Name what closes, not the kind of container it lives in. */}
      {closable && (
        <button className="ibtn" data-act="close" onClick={() => api.closePane(slot)}
          title={`Close ${subjectOf(activeId)?.ref ?? S.label}`}>{I.close}</button>
      )}
    </div>
  );

  return (
    <div className="paneshell">
      <div className="pane" data-slot={slot}>
        {multi ? (
          <div className="tabs" role="tablist" aria-label={`${slot} tabs`} ref={tabsRef}
            onKeyDown={(e) => {
              /* Roving focus: one tab stop per strip, arrows move and activate. */
              const step = e.key === 'ArrowRight' ? 1 : e.key === 'ArrowLeft' ? -1 : e.key === 'Home' ? -ids.length : e.key === 'End' ? ids.length : 0;
              if (!step) return;
              e.preventDefault();
              const i = Math.max(0, Math.min(ids.length - 1, ids.indexOf(activeId) + step));
              api.setActive(slot, ids[i]);
              e.currentTarget.querySelector(`[data-tab="${ids[i]}"]`)?.focus();
            }}>
            <SlideHighlight box={tabHl} id={`tabs-highlight-${slot}`} />
            {ids.map(id => (
              <button key={id} className="tab" role="tab" data-tab={id} aria-selected={String(id === activeId)}
                tabIndex={id === activeId ? 0 : -1} onClick={() => api.setActive(slot, id)}
                onPointerDown={api.startDrag(id, slot)}>
                {/* A tab has room for one thing; the reference is the thing
                    that distinguishes it from its neighbours. */}
                <span className="t" title={subjectOf(id) ? `${subjectOf(id).kind} ${subjectOf(id).ref}` : SURFACES[id].label}>
                  {subjectOf(id)?.ref ?? SURFACES[id].label}
                </span>
                <span className="x" onClick={(e) => { e.stopPropagation(); api.closePanel(slot, id); }}>×</span>
              </button>
            ))}
            {actions}
          </div>
        ) : (
          <div className="pane-head" data-od-id={`pane-head-${slot}`} onPointerDown={api.startDrag(activeId, slot)}>
            <PaneTitle id={activeId} />{actions}
          </div>
        )}
        <div className="pane-body" ref={bodyRef} data-active={activeId}>
          <S.Body api={api} active={api.centreActive} onPick={api.openCentre} />
        </div>
      </div>
    </div>
  );
}

/* The rail carries the same two vocabularies as the nav, separated by a rule:
   views on top with the current marker, panels below with open/closed dots. */
/*
  One navigation, two widths. `wide` swaps icons for the full labelled list —
  it is the same component and the same items, not a different surface. The
  toggle sits at the foot of the rail, where collapse controls live in every
  product that has one, and out of the way of the first destination.
*/
/*
  Cross-fade, not swap.

  The two faces are laid out differently — icons stacked at 72px, a labelled
  list at 248 — so there is no honest way to morph one into the other. Both
  stay mounted, stacked, and trade places over the same duration the width
  animates. Replacing one with the other mid-animation is what made this jar:
  the contents reflowed instantly inside a container that was still moving.
*/
function useCrossFade(ref, on, enabled) {
  const first = React.useRef(true);
  React.useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    if (first.current) { first.current = false; el.style.opacity = on ? '1' : '0'; return; }
    el.style.opacity = on ? '1' : '0';
    if (!enabled) return;
    el.animate([{ opacity: on ? 0 : 1 }, { opacity: on ? 1 : 0 }],
      { duration: window.__MOTION_MS || 280, easing: EASE });
  }, [on]);
}

function Rail({ api, compact, wide }) {
  const sections = [
    ['primary', NAV_ITEMS.filter(n => api.sectionOf(n.id) === 'primary')],
    ['panel', NAV_ITEMS.filter(n => api.sectionOf(n.id) === 'panel')],
  ];
  const iconsRef = useRef(null);
  const listRef = useRef(null);
  useCrossFade(iconsRef, !wide, api.anim);
  useCrossFade(listRef, wide, api.anim);
  const railHl = useSlidingHighlight(iconsRef, '.railbtn[aria-current="page"]',
    `${api.centreActive}|${compact}|${sections.map(([, i]) => i.map(n => n.id).join()).join('/')}`);

  return (
    <div className="paneshell">
      <div className="railstack" data-wide={String(wide)}>
        {/* A face that is fading out must not be reachable while it fades:
            `inert` takes it out of tab order, hit-testing and the a11y tree in
            one attribute, so an invisible navigation cannot be operated. */}
        <div className="railface rail wide" ref={listRef} data-od-id="nav-wide"
          aria-hidden={String(!wide)} inert={!wide ? '' : undefined}>
          <div className="railscroll"><Nav api={api} /></div>
          <button className="navtoggle" data-od-id={wide ? 'nav-toggle' : undefined} tabIndex={wide ? 0 : -1}
            title="Collapse navigation" aria-expanded="true" onClick={() => api.setNavWide(false)}>
            {I.collapseNav}<span>Collapse</span>
          </button>
        </div>

        {/* rail face */}
        <div className="railface rail" ref={iconsRef} data-compact={String(compact)}
          aria-hidden={String(wide)} inert={wide ? '' : undefined}>
        <SlideHighlight box={railHl} id="rail-highlight" />
        {sections.map(([kind, items], gi) => (
          <React.Fragment key={kind}>
            {gi > 0 && items.length > 0 && <div className="railsep" role="separator" />}
            {items.map(n => {
              const s = SURFACES[n.id];
              const view = kind === 'primary';
              const current = view && api.centreActive === n.id;
              const open = view ? api.isCentreOpen(n.id) : api.isPanelOpen(n.id);
              return (
                <button key={n.id} className="railbtn" data-kind={view ? 'view' : 'panel'} data-open={String(open)}
                  aria-current={current ? 'page' : undefined}
                  aria-checked={view ? undefined : String(open)} role={view ? undefined : 'switch'}
                  title={view ? n.label : (open ? 'Close ' : 'Open ') + n.label}
                  data-rail={n.id} onClick={() => view ? api.openCentre(n.id) : api.toggleSurface(n.id)}>
                  <span className="mk" aria-hidden="true" />
                  <span className="gl">{s ? s.glyph : '·'}</span><span className="tx">{n.label.split(' ')[0]}</span>
                </button>
              );
            })}
          </React.Fragment>
        ))}
        <button className="navtoggle" data-od-id={wide ? undefined : 'nav-toggle'} tabIndex={wide ? -1 : 0}
          title="Expand navigation" aria-expanded="false" onClick={() => api.setNavWide(true)}>
          {I.expandNav}
        </button>
        </div>
      </div>
    </div>
  );
}

/*
  One starting layout, not a menu of them. The states the presets used to
  select are all reachable from here through the product's own controls:

    nav expanded        the toggle at the foot of the rail, or drag its edge
    rail + panel        a nav switch, or the pane's collapse-to-rail control
    tabbed right pane   open a second right-hand panel from the nav
    focus               the focus control in the top bar, Esc to leave

  An operator moves between these during a session; they were never four
  different products.
*/
const START_LAYOUT = {
  centre: { panels: ['floor', 'board', 'runway'], active: 'floor' },
  left: { panels: ['attention'], size: 360 },
  right: { panels: ['inspector'], size: 430 },
};
/* Everything hidden; the centre keeps whatever it had. */
const FOCUS_LAYOUT = (from) => ({
  ...from,
  left: { ...from.left, mode: 'hidden' },
  right: { ...from.right, mode: 'hidden' },
  bottom: { ...from.bottom, panels: [], active: null },
});

/*
  The assistant: one panel, two modes of each other — Ask (typed) and Voice
  (spoken) — each one press from the other in the head. No transport exists
  yet, so a question is kept and the panel says plainly that nothing is
  connected; it never fakes an answer (tracker A12 step 3).
*/
function AssistantPanel({ mode, about, setMode, close }) {
  const [text, setText] = useState('');
  const [asked, setAsked] = useState([]);
  const ask = () => { const t = text.trim(); if (!t) return; setAsked(a => [...a, t]); setText(''); };
  return (
    <>
      <div className="pane-head">
        <span className="pane-title">{mode === 'ask' ? 'Assistant' : 'Listening'}</span>
        <span className="modes" role="group" aria-label="Assistant mode">
          <button type="button" aria-pressed={mode === 'ask'} data-od-id="assistant-mode-ask" onClick={() => setMode('ask')}>? Ask</button>
          <button type="button" aria-pressed={mode === 'voice'} data-od-id="assistant-mode-voice" onClick={() => setMode('voice')}>◉ Voice</button>
        </span>
        <div className="pane-actions">
          <button className="ibtn" data-act="close" title="Close the assistant" data-od-id="orb-panel-close" onClick={close}>{I.close}</button>
        </div>
      </div>
      <div className="assistant" data-mode={mode} data-od-id="assistant">
        {about && <div className="about" data-od-id="assistant-about">About <b>{about}</b></div>}
        {mode === 'ask' ? (
          <>
            {asked.map((t, i) => <div className="msg me" key={i}>{t}</div>)}
            <div className="msg quiet">No assistant is connected yet. What you ask is kept here until one is.</div>
            <form className="askrow" onSubmit={(e) => { e.preventDefault(); ask(); }}>
              <input value={text} onChange={(e) => setText(e.target.value)} placeholder={about ? `Ask about ${about}…` : 'Ask the factory…'}
                aria-label="Question" data-od-id="assistant-input" autoFocus />
              <button type="submit" className="btn" data-od-id="assistant-send">Ask</button>
            </form>
          </>
        ) : (
          <div className="voice">
            <div className="meter" aria-hidden="true"><i /><i /><i /><i /><i /></div>
            <div className="msg quiet">Voice input is not connected yet. Nothing is being recorded.</div>
            <button type="button" className="btn" onClick={() => setMode('ask')}>Type instead</button>
          </div>
        )}
      </div>
    </>
  );
}

function App() {
  /* One reducer, from shell/src/layout.ts. Every shape change goes through it,
     so the Move menu, the nav, the region toggles and (later) drag-and-drop
     cannot each invent their own rules. */
  /* The workspace wraps the layout with per-view memory (§5.2.3): navigating
     between views stores and recalls each view's side panes. */
  const [workspace, dispatch] = React.useReducer(
    (ws, a) => reduceWorkspace(SURFACES, ws, a),
    'start',
    () => ({ layout: fromPreset(SURFACES, START_LAYOUT), views: {}, placed: {} }),
  );
  const layout = workspace.layout;
  const { centre, left, right, bottom } = layout;
  /* Settings are preferences, not layout, so they live outside ShellState.
     Dark is the default. No colour-scheme query, for the same reason there is
     no motion query: a host signal deciding how the product opens is how it
     opened wrong. The operator's stored choice still wins over the default. */
  const [settings, setSettings] = useState(() => loadSettings(store));
  /* Motion is on. Not a setting, not a toggle, not read from anywhere. */
  const anim = true;
  /* Focus mode remembers the layout it collapsed, so leaving restores exactly
     what was there. Deliberately not persisted: a reload should not resume
     into a shell with everything hidden. */
  const [focusFrom, setFocusFrom] = useState(null);
  useEffect(() => {
    document.documentElement.dataset.theme = settings.theme;
    saveSettings(store, settings);
  }, [settings]);
  const [simWidth, setSimWidth] = useState(0);
  const [drawer, setDrawer] = useState(false);
  const drawerTriggerRef = useRef(null);
  const drawerFallbackRef = useRef(null);
  const [bp, setBp] = useState('xl');
  const [stageW, setStageW] = useState(0);
  const stageRef = useRef(null);
  const workRef = useRef(null);
  const leftRef = useRef(null);
  const railRef = useRef(null);
  const rightRef = useRef(null);
  const bottomRef = useRef(null);
  const dragging = useRef(false);

  /* Focus mode: collapse every pane to the centre, and restore exactly what
     was there on the way out. Esc leaves, so the mode always has a visible
     exit and a keyboard one. */
  const focused = !!focusFrom;
  const toggleFocus = useCallback(() => {
    setFocusFrom(prev => {
      if (prev) { dispatch({ type: 'restore', state: prev }); return null; }
      dispatch({ type: 'restore', state: FOCUS_LAYOUT(layout) });
      return layout;
    });
    setDrawer(false);
  }, [layout]);

  /* The command palette: Ctrl+P / ⌘K, or the search control. It names an
     intent and the same api the nav uses carries it out, so a surface opened
     from here is operator-requested and may dock directly (§5.1.1). */
  const [palette, setPalette] = useState(false);
  const paletteTriggerRef = useRef(null);

  /* Inspection: what is selected, and whether the inspector floats or is
     docked. The form follows the trigger (§5.1.1): content selection floats,
     pin docks through the placement reducer, a command docks directly. */
  const [inspect, setInspect] = useState(NO_INSPECTION);
  /* At the narrow breakpoint the side panes have no width: a docked inspector
     there is invisible, so the model must treat those slots as unavailable. */
  const unavailable = bp === 'sm' ? ['left', 'right'] : [];
  const runInspect = (action) => {
    const plan = inspectPlan(layout, inspect, action, unavailable);
    if (plan.layoutAction) dispatch(plan.layoutAction);
    setInspect(plan.inspect);
  };
  const inspected = useMemo(() => lookupItem(inspect.ref), [inspect.ref]);
  const inspectMode = computeInspectMode(layout, inspect, unavailable);

  /* The float remembers where it was left: an offset from its anchored home,
     clamped to the stage, saved through the storage port. Dragging is by the
     header; at narrow widths the float is a bottom sheet and does not drag. */
  const [floatPos, setFloatPos] = useState(() => loadFloat(store));
  const [floatDrag, setFloatDrag] = useState(false);
  useEffect(() => { saveFloat(store, floatPos); }, [floatPos]);
  const startFloatDrag = (e) => {
    if (e.button !== 0 || e.target.closest('button') || bp === 'sm') return;
    const panel = e.currentTarget.closest('.od-overlay-panel');
    const wrap = panel?.parentElement;
    if (!panel || !wrap) return;
    const r = panel.getBoundingClientRect(), b = wrap.getBoundingClientRect(), cs = getComputedStyle(wrap);
    const inset = { l: parseFloat(cs.paddingLeft) || 0, r: parseFloat(cs.paddingRight) || 0, t: parseFloat(cs.paddingTop) || 0, b: parseFloat(cs.paddingBottom) || 0 };
    // How far it may still travel from here, in each direction.
    const range = {
      minDx: floatPos.dx - (r.left - b.left - inset.l), maxDx: floatPos.dx + (b.right - inset.r - r.right),
      minDy: floatPos.dy - (r.top - b.top - inset.t), maxDy: floatPos.dy + (b.bottom - inset.b - r.bottom),
    };
    const startX = e.clientX, startY = e.clientY, from = floatPos;
    setFloatDrag(true);
    const move = (ev) => setFloatPos(clampFloat({ dx: from.dx + ev.clientX - startX, dy: from.dy + ev.clientY - startY }, range));
    const up = () => { setFloatDrag(false); window.removeEventListener('pointermove', move); window.removeEventListener('pointerup', up); };
    window.addEventListener('pointermove', move); window.addEventListener('pointerup', up);
    e.preventDefault();
  };

  /* The orb (§5.3): above the panes and floats, below the P0 dialog. It keeps
     its place as an offset from its home corner, under its own key. Its ring
     is built from the same model the palette uses; in focus mode and at the
     smallest size the ring carries the views itself. Ask and Voice open one
     assistant panel in two modes; nothing is connected behind it yet, and it
     says so. */
  const [orbPos, setOrbPos] = useState(() => loadFloat(store, ORB_KEY));
  useEffect(() => { saveFloat(store, orbPos, ORB_KEY); }, [orbPos]);
  const [assistant, setAssistant] = useState({ open: false, mode: 'ask', about: null });
  // Focus mode hides the nav; at the smallest size it is a drawer. Either way the orb carries it.
  const navHidden = left.mode === 'hidden' || bp === 'sm';
  const orbEntries = useMemo(() => orbMenu({
    registry: SURFACES, state: layout, order: NAV_ORDER, navHidden,
    selection: inspect.ref ? { ref: inspect.ref, title: inspected?.title } : null,
  }), [layout, navHidden, inspect.ref, inspected]);
  const runOrb = (id) => {
    const intent = orbIntentOf(orbEntries, id);
    if (!intent) return;
    if (intent.kind === 'palette') setPalette(true);
    else if (intent.kind === 'assistant') setAssistant({ open: true, mode: intent.mode, about: intent.about ?? null });
    else runCommand({ intent });
  };
  // The orb needs the stage element to roam in; render it once the stage exists.
  const [stageReady, setStageReady] = useState(false);
  useLayoutEffect(() => { setStageReady(!!stageRef.current); }, []);

  /* P0 interruption: the pause is a fact the dialog reports. Minimising keeps
     a persistent strip; only a resume with a recorded justification ends it. */
  const [p0, dispatchP0] = React.useReducer(interruptReduce, NO_INTERRUPT);
  const [confirmAbort, setConfirmAbort] = useState(false);
  const runP0 = (action) => { setConfirmAbort(false); dispatchP0(action); };

  /* Environment scope is a choice; the attention indicator opens a short list
     whose rows select the item, so the inspector shows it. Both are anchored
     Popovers — the first two consumers of that primitive. */
  const [env, setEnv] = useState(DATA.environments.current);
  const currentEnv = DATA.environments.items.find(e => e.id === env) ?? DATA.environments.items[0];
  const needsYou = DATA.attention.items.filter(i => i.needsYou);
  const [attnOpen, setAttnOpen] = useState(false);
  usePaletteShortcut(useCallback(() => setPalette(p => !p), []));
  const commands = useMemo(() => buildCommands({
    registry: SURFACES, state: layout, order: NAV_ORDER,
    workItems: WORK_ITEMS, runs: RUNS, theme: settings.theme, focused,
  }), [layout, settings.theme, focused]);

  useEffect(() => {
    if (!focused) return;
    const onKey = (e) => {
      if (e.key === 'Escape' && !e.defaultPrevented && !drawer && !palette) toggleFocus();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [focused, toggleFocus, drawer, palette]);

  /* DESIGN.md §16.3: Alt+1/2/3 jump to the views in nav order. Alt keeps clear
     of the palette's Ctrl/⌘ and of typing in fields. */
  useEffect(() => {
    const onKey = (e) => {
      if (!e.altKey || e.ctrlKey || e.metaKey || e.shiftKey || !/^[1-9]$/.test(e.key)) return;
      const views = NAV_ORDER.filter(id => sectionOf(SURFACES, layout, id) === 'primary');
      const id = views[Number(e.key) - 1];
      if (!id) return;
      e.preventDefault();
      dispatch({ type: 'open', id, to: 'centre' });
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [layout]);

  // Resizing back to a persistent rail must not leave a modal over it.
  useEffect(() => { if (bp !== 'sm') setDrawer(false); }, [bp]);

  /* breakpoints measured from the stage, not the window */
  useEffect(() => {
    const el = stageRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => { setBp(breakpointFor(el.clientWidth)); setStageW(el.clientWidth); });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const coarse = useMemo(() => window.matchMedia('(pointer: coarse)').matches, []);
  const railW = railWidthFor(bp, coarse);
  const sideWidth = (slot) => computeSideWidth(SURFACES, layout, slot, bp, railW);

  /* The view layer never mutates layout state — it names an intent and the
     tested reducer decides what that means. */
  const api = {
    anim,
    centreActive: centre.active,
    openCentre: (id) => dispatch({ type: 'open', id, to: 'centre' }),
    openPanel: (id) => dispatch({ type: 'open', id }),
    setActive: (slot, id) => dispatch({ type: 'activate', slot, id }),
    closePanel: (slot, id) => dispatch({ type: 'closeSurface', id }),
    closePane: (slot) => dispatch({ type: 'closePane', slot }),
    setMode: (slot, mode) => dispatch({ type: 'setMode', slot, mode }),
    toggleSurface: (id) => dispatch({ type: 'toggleSurface', id }),
    /* No control calls this yet — drag-and-drop will. The rule it enforces
       (accepts) is in the reducer, so a drop handler decides *where*, never
       *whether*. Kept deliberately: the menu that used to call it was the
       wrong affordance, not the wrong mechanism. */
    moveSurface: (id, to) => dispatch({ type: 'move', id, to }),
    placementOf: (id) => placementOf(layout, id),
    sectionOf: (id) => sectionOf(SURFACES, layout, id),
    isCentreOpen: (id) => centre.panels.includes(id),
    isPanelOpen: (id) => isPanelOpen(layout, id),
    acceptedSlots: (id) => acceptedSlots(SURFACES[id]),
    /* Navigation width. Not a surface, not a panel — the rail gets wider and
       swaps icons for labels. Only available when no panel shares the column,
       since expanded nav and a panel side by side is two fat columns. */
    setNavWide: (on) => dispatch({ type: 'setNavWide', wide: on }),
    /* Drag a tab or header to another region; the reducer decides the rest.
       Bound lazily: startDrag is declared further down this function. */
    startDrag: (id, from) => (e) => startDrag(id, from)(e),
    /* The design artifact has no window host; the desktop app binds Tauri. */
    popOut: noPopOut,
    /* Content selection and the docked inspector's way back out. */
    inspected,
    select: (ref) => runInspect({ type: 'select', ref, via: 'content' }),
    unpin: () => runInspect({ type: 'unpin' }),
    canFloat: inspect.ref !== null,
  };

  const runCommand = (command) => {
    const i = command.intent;
    if (i.kind === 'view') api.openCentre(i.id);
    else if (i.kind === 'toggle') api.toggleSurface(i.id);
    else if (i.kind === 'reveal') dispatch({ type: 'open', id: i.id });
    else if (i.kind === 'move') api.moveSurface(i.id, i.to);
    else if (i.kind === 'inspect') runInspect({ type: 'select', ref: i.ref, via: 'command' });
    else if (i.kind === 'theme') setSettings(s => ({ ...s, theme: nextTheme(s.theme) }));
    else if (i.kind === 'focus') toggleFocus();
  };

  /* What was last in the left region, so toggling it back restores it. */
  const lastLeft = useRef(START_LAYOUT.left.panels);

  /*
    A region toggle names the thing it will show or hide, not the furniture it
    lives in — "Show Attention", not "Left panel". The region's own name is
    the fallback only when nothing has ever been in it.
  */
  /* Shown means there is something in the region and the region is visible.
     A hidden region still holds its panels, so the toggle reads Hide/Show and
     names what is in it — by subject where there is one — not a fixed label. */
  const regionShown = (slot) => layout[slot].panels.length > 0 && layout[slot].mode !== 'hidden';
  const regionTitle = (slot) => {
    const pane = layout[slot];
    const remembered = slot === 'left' ? lastLeft.current
      : slot === 'right' ? ['inspector']
      : ['recorder'];
    const ids = pane.panels.length ? pane.panels : remembered;
    const names = ids.map(id => subjectFor(id, inspected)?.ref ?? SURFACES[id]?.label).filter(Boolean);
    if (!names.length) return `Show ${slot}`;
    return `${regionShown(slot) ? 'Hide' : 'Show'} ${names.join(' · ')}`;
  };

  /*
    The region toggles show and hide *panels*. Navigation is not a panel: it is
    the shell's own furniture and the only way back to everything else, so no
    region toggle may take it away. Focus mode is the single exception, and it
    is an explicit mode with a visible exit and Esc.
  */
  const toggleRegion = (r) => {
    if (r === 'left') {
      if (left.panels.length) { lastLeft.current = left.panels; dispatch({ type: 'closePane', slot: 'left' }); }
      else for (const id of (lastLeft.current.length ? lastLeft.current : ['attention'])) {
        dispatch({ type: 'open', id, to: 'left' });
      }
      return;
    }
    /* The bottom region is transient: toggling it on opens the recorder, which
       is the only bottom-shaped surface we have. */
    if (r === 'bottom' && !bottom.panels.length) { dispatch({ type: 'open', id: 'recorder', to: 'bottom' }); return; }
    if (r === 'right' && right.mode === 'hidden' && !right.panels.length) { dispatch({ type: 'open', id: 'inspector', to: 'right' }); return; }
    dispatch({ type: 'toggleRegion', slot: r });
  };

  /* Keyboard resize on a focused grip: arrows move the edge in 16px steps.
     The same reducer clamps, so keyboard and pointer cannot disagree. */
  const keyResize = (which) => (e) => {
    const grow = which === 'bottom' ? (e.key === 'ArrowUp' ? 1 : e.key === 'ArrowDown' ? -1 : 0)
      : which === 'left' ? (e.key === 'ArrowRight' ? 1 : e.key === 'ArrowLeft' ? -1 : 0)
      : (e.key === 'ArrowLeft' ? 1 : e.key === 'ArrowRight' ? -1 : 0);
    if (!grow) return;
    e.preventDefault();
    // Relative, so key repeat faster than a render still adds up.
    dispatch(which === 'bottom'
      ? { type: 'resizeBy', slot: which, delta: grow * 16, min: 80, max: 420 }
      : { type: 'resizeBy', slot: which, delta: grow * 16 });
  };

  /* Drag-rearrange (§5.2.4). The view draws the zones and decides *where* the
     pointer is; what a drop means is the reducer's `move`, which also enforces
     which slots a surface accepts. A held tab or pane header becomes a drag
     after 6px; Escape cancels. */
  const [drag, setDrag] = useState(null);
  const zoneRects = () => {
    const s = stageRef.current.getBoundingClientRect(), w = workRef.current.getBoundingClientRect();
    const rel = (x, y, wd, ht) => ({ left: x - s.left, top: y - s.top, width: wd, height: ht });
    const bh = Math.max(120, bottomHeight(layout));
    const lwz = Math.max(160, sideWidth('left')), rwz = Math.max(160, sideWidth('right'));
    const mainH = (w.bottom - w.top) + bottomHeight(layout) - bh;
    return {
      left: rel(w.left, w.top, lwz, mainH),
      centre: rel(w.left + lwz, w.top, (w.right - w.left) - lwz - rwz, mainH),
      right: rel(w.right - rwz, w.top, rwz, mainH),
      bottom: rel(w.left, w.top + mainH, w.right - w.left, bh),
    };
  };
  const zoneAt = (zones, x, y) => {
    const s = stageRef.current.getBoundingClientRect();
    const px = x - s.left, py = y - s.top;
    return Object.keys(zones).find(z => { const r = zones[z]; return px >= r.left && px <= r.left + r.width && py >= r.top && py <= r.top + r.height; }) ?? null;
  };
  const startDrag = (id, from) => (e) => {
    if (e.button !== 0 || e.target.closest('.pane-actions, .x') || bp === 'sm') return;
    const label = subjectFor(id, inspected)?.ref ?? SURFACES[id].label;
    const zones = zoneRects();
    const allowed = api.acceptedSlots(id);
    const sx = e.clientX, sy = e.clientY;
    let live = false;
    const move = (ev) => {
      if (!live) { if (Math.hypot(ev.clientX - sx, ev.clientY - sy) < 6) return; live = true; }
      setDrag({ id, from, label, zones, allowed, x: ev.clientX, y: ev.clientY, over: zoneAt(zones, ev.clientX, ev.clientY) });
    };
    const end = () => {
      window.removeEventListener('pointermove', move); window.removeEventListener('pointerup', up);
      window.removeEventListener('keydown', key, true);
      setDrag(null);
    };
    const up = (ev) => {
      const z = live ? zoneAt(zones, ev.clientX, ev.clientY) : null;
      end();
      if (z && z !== from && allowed.includes(z)) api.moveSurface(id, z);
    };
    const key = (ev) => { if (ev.key === 'Escape') { ev.stopPropagation(); end(); } };
    window.addEventListener('pointermove', move); window.addEventListener('pointerup', up);
    window.addEventListener('keydown', key, true);
  };

  /* pointer resize: suspend the transition so the pane tracks the cursor 1:1 */
  const startResize = (which) => (e) => {
    if (e.button !== 0) return;
    const startX = e.clientX, startY = e.clientY;
    const s0 = layout[which].size;
    dragging.current = true;
    e.currentTarget.classList.add('on');
    const target = e.currentTarget;
    const move = (ev) => {
      const size = which === 'left' ? s0 + (ev.clientX - startX)
        : which === 'right' ? s0 - (ev.clientX - startX)
        : clamp(s0 - (ev.clientY - startY), 80, 420);
      dispatch({ type: 'resize', slot: which, size });
    };
    const up = () => {
      dragging.current = false;
      target.classList.remove('on');
      window.removeEventListener('pointermove', move); window.removeEventListener('pointerup', up);
    };
    window.addEventListener('pointermove', move); window.addEventListener('pointerup', up);
    e.preventDefault();
  };

  useEffect(() => { saveWorkspace(store, workspace); }, [workspace]);
  /* Lets the self-test assert against model state, not just pixels. */
  useEffect(() => { window.__placement = (id) => placementOf(layout, id); }, [layout]);

  const navWide = left.mode === 'nav';

  /* Drag the rail's edge to change navigation width. It snaps rather than
     resizing freely: there are two navigation widths, not a continuum. */
  const startNavDrag = (e) => {
    if (e.button !== 0) return;
    const startX = e.clientX;
    const target = e.currentTarget;
    target.classList.add('on');
    const move = (ev) => {
      const dx = ev.clientX - startX;
      if (dx > 40) api.setNavWide(true);
      else if (dx < -40) api.setNavWide(false);
    };
    const up = () => {
      target.classList.remove('on');
      window.removeEventListener('pointermove', move); window.removeEventListener('pointerup', up);
    };
    window.addEventListener('pointermove', move); window.addEventListener('pointerup', up);
    e.preventDefault();
  };

  const navW = (navWide ? NAV_WIDTH : railW) + RAIL_GAP;
  /* The centre keeps its floor (CENTRE_MIN); side panes give way in proportion.
     The rule was in the model and tested, but the view never applied it — at
     the old scale the centre always had room. At 125 % it does not. */
  const fitted = fitToViewport(stageW || Infinity, sideWidth('left'), sideWidth('right'));
  const lw = fitted.left, rw = fitted.right;
  const bh = bottomHeight(layout);
  const leftRail = left.mode === 'rail' || left.mode === 'nav';
  useAnimatedSize(leftRef, lw, 'width', anim, dragging);
  /* The rail wrapper animates on the same clock as the column it sits in. */
  useAnimatedSize(railRef, navW, 'width', anim, dragging);
  useAnimatedSize(rightRef, rw, 'width', anim, dragging);
  useAnimatedSize(bottomRef, bh, 'height', anim, dragging);

  return (
    <InspectedContext.Provider value={inspected}>
    <div className="outer">
      {/* The only scaffolding left. Layout states are reached through the
          product's own controls, not through a preset switcher. */}
      <div className="lab" data-od-id="lab">
        <div className="grp"><span className="lbl">Viewport</span>
          {[0, 1600, 1280, 1024, 820].map(w => <button key={w} className="lb" data-w={w} aria-pressed={String(simWidth === w)} onClick={() => setSimWidth(w)}>{w === 0 ? 'Full' : w}</button>)}
        </div>
        {/* System-raised events have no product control; the lab raises one. */}
        <div className="grp"><span className="lbl">Simulate</span>
          <button className="lb" data-od-id="raise-p0" onClick={() => runP0({ type: 'raise' })}>Raise P0</button>
        </div>
        <span className="note" id="bpnote">{bp} · {coarse ? 'touch' : 'mouse'} · rail {railW} · L {lw} · C flex · R {rw}</span>
      </div>

      <div className="stagewrap">
        <div className="stage" ref={stageRef} data-anim={String(anim)} data-bp={bp} data-dragging={String(!!drag)} style={{ maxWidth: simWidth ? simWidth + 'px' : '100%' }} data-od-id="app">
          <header className="topbar" data-od-id="topbar">
            <div className="brand">
              {bp === 'sm' && <button ref={drawerTriggerRef} className="ibtn nav-trigger" aria-label="Open navigation" aria-haspopup="dialog"
                aria-expanded={drawer} aria-controls={drawer ? 'drawer' : undefined}
                onClick={() => setDrawer(true)} data-od-id="hamburger">{I.menu}</button>}
              <i /><span>Software Factory</span>
            </div>
            <button ref={paletteTriggerRef} type="button" className="search" data-od-id="search"
              aria-haspopup="dialog" aria-expanded={palette} aria-controls={palette ? 'command-palette' : undefined}
              onClick={() => setPalette(true)}>
              <span className="mono" aria-hidden="true">⌕</span><span>Search work, runs, panels, actions</span>
              <span className="mono" style={{ marginLeft: 'auto', color: 'var(--fg-faint)' }}>Ctrl P</span>
            </button>
            <div className="topright">
              <ChoiceMenu id="env-scope" label="Environment scope" triggerClassName="pill"
                triggerTitle="Environment scope" portalContainer={stageRef.current}
                trigger={<>{currentEnv.label} · <b style={{ color: SC[currentEnv.state] }}>{currentEnv.health}</b></>}
                choices={DATA.environments.items.map(e => ({ id: e.id, label: e.label, hint: e.health, tone: SC[e.state] }))}
                selected={env} onSelect={setEnv} />
              <Popover id="needs-you" label="Needs you" triggerClassName="pill" triggerTitle="What needs you"
                trigger={`${needsYou.length} need you`} isOpen={attnOpen} onOpenChange={setAttnOpen}
                portalContainer={stageRef.current}>
                <div className="od-popover-head">Needs you · {needsYou.length}</div>
                {needsYou.map(i => (
                  <button key={i.id} type="button" className="od-row" data-od-id={`needs-you-${i.id}`}
                    onClick={() => { setAttnOpen(false); api.select(i.id); }}>
                    <i className="dot" style={{ background: SC[i.state], marginTop: 5 }} />
                    <span style={{ minWidth: 0 }}>{i.headline}<span className="od-row-sub">{i.detail}</span></span>
                  </button>
                ))}
              </Popover>
              <div className="rtog" data-od-id="region-toggles">
                <button data-region="left" title={regionTitle('left')} aria-pressed={String(regionShown('left'))} onClick={() => toggleRegion('left')}>◧</button>
                <button data-region="right" title={regionTitle('right')} aria-pressed={String(regionShown('right'))} onClick={() => toggleRegion('right')}>◨</button>
                <button data-region="bottom" title={regionTitle('bottom')} aria-pressed={String(regionShown('bottom'))} onClick={() => toggleRegion('bottom')}>◪</button>
              </div>
              <button ref={drawerFallbackRef} className="ibtn" data-od-id="focus-toggle" aria-pressed={String(focused)}
                title={focused ? 'Leave focus mode (Esc)' : 'Focus mode'}
                onClick={toggleFocus}>{focused ? I.unfocus : I.focus}</button>
              {/* One subtle control; the glyph shows the theme it switches to. */}
              <button className="ibtn" data-od-id="theme-toggle" data-theme-now={settings.theme}
                title={settings.theme === 'dark' ? 'Switch to light' : 'Switch to dark'}
                onClick={() => setSettings(s => ({ ...s, theme: nextTheme(s.theme) }))}>
                {settings.theme === 'dark' ? I.sun : I.moon}
              </button>
            </div>
          </header>

          {/* CriticalBanner: a persistent strip, never an overlay. It stays
              while the pause is in force and the dialog is out of the way. */}
          {showsBanner(p0) && (
            <div className="critical" role="status" data-od-id="critical-banner">
              <span className="sev">‼ P0</span>
              <span className="msg">{DATA.p0.title}</span>
              <span className="mono">paused since {DATA.p0.raisedAt}</span>
              {/* Layout controls are icons with a tooltip, never a word button. */}
              <button className="ibtn" data-od-id="critical-open" title="Open the P0 decision" onClick={() => runP0({ type: 'reopen' })}>{I.maximise}</button>
            </div>
          )}

          <div className="work" ref={workRef} data-od-id="work">
            <div className="col side" data-side="left" ref={leftRef} style={{ width: lw }}>
              <div style={{ display: 'flex', height: '100%', minWidth: lw || 1 }}>
                {leftRail && (
                  /* This wrapper's width used to be set instantly while the
                     column beside it animated, so the rail's contents
                     reflowed inside a container that was still moving. It
                     animates on the same clock now. */
                  <div ref={railRef} style={{ width: navW, flex: '0 0 auto', ['--nav-width']: NAV_WIDTH + 'px' }}>
                    <Rail api={api} compact={railW === RAIL_COMPACT} wide={navWide} />
                  </div>
                )}
                {/* Navigation and panel are independent: a panel shows because
                    it is open, at whichever navigation width is showing. */}
                {left.panels.length > 0 && (
                  <div style={{ flex: 1, minWidth: 0 }}><Pane slot="left" state={left} api={api} /></div>
                )}
              </div>
            </div>
            {/* Dragging the left edge snaps between the two navigation widths
                when nothing else shares the column, and resizes the panel when
                something does. The cursor carries the affordance either way. */}
            {lw > 0 && bp !== 'sm' && (
              <div className="grip" data-od-id="grip-left" role="separator" aria-orientation="vertical" tabIndex={0}
                aria-label={left.panels.length ? 'Resize the left pane' : 'Navigation width'}
                onPointerDown={left.panels.length ? startResize('left') : startNavDrag}
                onKeyDown={left.panels.length ? keyResize('left') : (e) => {
                  if (e.key === 'ArrowRight') { e.preventDefault(); api.setNavWide(true); }
                  if (e.key === 'ArrowLeft') { e.preventDefault(); api.setNavWide(false); }
                }} />
            )}

            <div className="col centre" data-side="centre"><Pane slot="centre" state={centre} api={api} /></div>

            {rw > 0 && bp !== 'sm' && <div className="grip" onPointerDown={startResize('right')} onKeyDown={keyResize('right')} data-od-id="grip-right"
              role="separator" aria-orientation="vertical" tabIndex={0} aria-label="Resize the right pane" />}
            <div className="col side" data-side="right" ref={rightRef} style={{ width: rw }}>
              <div style={{ height: '100%', minWidth: rw || 1 }}><Pane slot="right" state={right} api={api} /></div>
            </div>
          </div>

          {/* bottom is transient: it exists only while a bottom-shaped task is open */}
          <div className="bottom" ref={bottomRef} style={{ height: bh }} data-od-id="bottom">
            {bottom.panels.length > 0 && (
              <div style={{ height: '100%', display: 'flex', flexDirection: 'column' }}>
                <div className="grip" data-dir="v" onPointerDown={startResize('bottom')} onKeyDown={keyResize('bottom')} data-od-id="grip-bottom"
                  role="separator" aria-orientation="horizontal" tabIndex={0} aria-label="Resize the bottom pane" />
                <div style={{ flex: 1, minHeight: 0 }}><Pane slot="bottom" state={bottom} api={api} /></div>
              </div>
            )}
          </div>

          <Overlay isOpen={drawer} onOpenChange={setDrawer} id="drawer" label="Navigation"
            placement="left" portalContainer={stageRef.current}
            triggerRef={drawerTriggerRef} fallbackFocusRef={drawerFallbackRef}>
            <div className="pane-head"><span className="pane-title">Navigate</span>
              <button className="overlay-close" aria-label="Close navigation" data-od-id="drawer-close"
                onClick={() => setDrawer(false)}>{I.close}</button></div>
            <div className="od-overlay-body">
              <Nav api={{ ...api,
                openCentre: (id) => { api.openCentre(id); setDrawer(false); },
              }} />
            </div>
          </Overlay>

          <CommandPalette isOpen={palette} onOpenChange={setPalette} commands={commands} onRun={runCommand}
            portalContainer={stageRef.current} triggerRef={paletteTriggerRef} />

          {/* While something is held, every region shows where it could land. */}
          {drag && (
            <div className="dropzones" data-od-id="dropzones" aria-hidden="true">
              {Object.entries(drag.zones).map(([zone, r]) => (
                <div key={zone} className="dropzone" data-zone={zone} data-over={String(drag.over === zone)}
                  data-allowed={String(drag.allowed.includes(zone) && zone !== drag.from)}
                  style={{ left: r.left, top: r.top, width: r.width, height: r.height }}>
                  {zone === drag.from ? 'here' : SLOT_LABEL[zone]}
                </div>
              ))}
              <div className="drag-ghost" style={{ left: drag.x, top: drag.y }}>{drag.label}</div>
            </div>
          )}

          {/* Content-selected inspection floats first: the layout must not move
              because the operator looked at something. Non-modal, so the
              content underneath stays live and the next click just swaps the
              subject. Pin docks it; unpin on the docked pane lifts it back. */}
          <Overlay isOpen={inspectMode === 'floating'} modal={false} placement={bp === 'sm' ? 'bottom' : 'right'}
            onOpenChange={(open) => { if (!open) runInspect({ type: 'dismiss' }); }}
            id="inspector-float" label={`Work item ${inspect.ref ?? ''}`}
            panelStyle={bp === 'sm' ? undefined : { left: floatPos.dx, top: floatPos.dy }}
            portalContainer={stageRef.current} fallbackFocusRef={drawerFallbackRef}>
            <div className="pane-head" data-od-id="inspector-float-head"
              data-drag={bp === 'sm' ? undefined : (floatDrag ? 'on' : 'off')} onPointerDown={startFloatDrag}>
              <PaneTitle id="inspector" />
              <div className="pane-actions">
                <button className="ibtn" data-act="pin" title={`Pin ${inspect.ref ?? ''} to the right`}
                  onClick={() => runInspect({ type: 'pin' })}>{I.pin}</button>
                <button className="ibtn" data-act="close" title={`Close ${inspect.ref ?? ''}`}
                  onClick={() => runInspect({ type: 'dismiss' })}>{I.close}</button>
              </div>
            </div>
            <div className="od-overlay-body"><Inspector api={api} /></div>
          </Overlay>

          {/* The orb: draggable, flickable, press for the ring. Its assistant
              panel is the same non-modal float the inspector uses. */}
          {stageReady && (
            <FloatingOrb items={orbEntries} boundsRef={stageRef} ring="floating" label="Assistant"
              offset={orbPos} onMove={(p) => setOrbPos({ dx: p.dx, dy: p.dy })} onAction={runOrb}
              panelOpen={assistant.open} onPanelOpenChange={(open) => setAssistant(a => ({ ...a, open }))}
              renderPanel={(close) => (
                <AssistantPanel mode={assistant.mode} about={assistant.about}
                  setMode={(mode) => setAssistant(a => ({ ...a, mode }))} close={close} />
              )} />
          )}

          {/* P0 decision Dialog: modal, not dismissable — Escape and the scrim
              do nothing, because leaving is not a decision. */}
          <Overlay isOpen={p0.status === 'open'} onOpenChange={() => {}} modal placement="center"
            className="od-overlay-wide" isDismissable={false} isKeyboardDismissDisabled
            id="p0" role="alertdialog" label={`P0 · ${DATA.p0.title}`}
            portalContainer={stageRef.current} fallbackFocusRef={drawerFallbackRef}>
            <div className="p0-head" data-od-id="p0-head">
              <span className="sev">‼ P0</span>
              <span className="p0-title">{DATA.p0.title}</span>
              <button className="ibtn" data-act="minimize" title="Minimise — the pause stays in force everywhere"
                onClick={() => runP0({ type: 'minimize' })}>{I.minimise}</button>
            </div>
            <div className="od-overlay-body p0-body">
              <section><h5>What happened · {DATA.p0.raisedAt}</h5><p>{DATA.p0.what}</p></section>
              <section><h5>Why this is P0</h5><p>{DATA.p0.why}</p></section>
              <section><h5>Impact and potentially affected systems</h5>
                <div className="p0-scope">{DATA.p0.scope.map(c => <span key={c.label} className="p0-chip" data-hot={String(!!c.hot)}>{c.label}</span>)}</div>
              </section>
              <section><h5>Automatic containment</h5>
                <div className="p0-two">
                  <div className="p0-col" data-kind="paused"><h6>Paused — smallest safe scope</h6>
                    <ul>{[...DATA.p0.paused, ...(p0.expanded ? DATA.p0.pausedExpanded : [])].map(t => <li key={t}>{t}</li>)}</ul></div>
                  <div className="p0-col" data-kind="running"><h6>Still running</h6>
                    <ul>{DATA.p0.running.map(t => <li key={t}>{t}</li>)}</ul></div>
                </div>
                {p0.playbookRun && <p className="p0-status" data-od-id="p0-playbook-result"><b>{DATA.p0.playbook.id} executed</b> — {DATA.p0.playbook.result}</p>}
                {p0.aborted && <p className="p0-status" data-od-id="p0-abort-result">{DATA.p0.abortResult}</p>}
              </section>
              <section><h5>Evidence</h5>
                {DATA.p0.evidence.map(e => (
                  <div key={e.id} className="ev"><i className="dot" style={{ background: SC[e.state], marginTop: 4 }} /><span style={{ color: 'var(--fg)' }}>{e.text}</span><span className="fxm" style={{ marginLeft: 'auto', color: 'var(--fg-faint)' }}>{e.meta}</span></div>
                ))}
              </section>
            </div>
            <div className="p0-foot">
              <label className="p0-justify">
                <span>Resume justification — recorded to the audit log</span>
                <textarea value={p0.justification} onChange={e => runP0({ type: 'justify', text: e.target.value })}
                  placeholder={DATA.p0.justificationHint} aria-invalid={String(p0.needsJustification)} data-od-id="p0-justification" />
                {p0.needsJustification && <span className="p0-warn" role="alert" data-od-id="p0-warn">A recorded justification is required before the paused scope can resume.</span>}
              </label>
              <div className="p0-actions">
                <span className="p0-note">Minimising does not resume work — the pause stays visible everywhere until it is resolved.</span>
                <button className="btn" data-act="expand" disabled={p0.expanded} onClick={() => runP0({ type: 'expand' })}>Expand containment</button>
                <button className="btn" data-act="playbook" disabled={p0.playbookRun} onClick={() => runP0({ type: 'playbook' })}>Run playbook {DATA.p0.playbook.id}</button>
                {/* An operation: confirm on the same control, then audit. */}
                <button className="btn" data-act="abort" data-confirm={String(confirmAbort)} disabled={p0.aborted}
                  onClick={() => { if (confirmAbort) runP0({ type: 'abort' }); else setConfirmAbort(true); }}>
                  {confirmAbort ? 'Confirm abort' : 'Abort affected work'}
                </button>
                <button className="btn pri" data-act="resume" onClick={() => runP0({ type: 'resume' })}>Resume…</button>
              </div>
            </div>
          </Overlay>
        </div>
      </div>
    </div>
    </InspectedContext.Provider>
  );
}

export { App, store };
