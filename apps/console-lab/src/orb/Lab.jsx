import React, { useState, useRef } from 'react';
import { FloatingOrb } from '@open-software-factory/console-ui/orb';

const ITEMS = [
  { id: 'ask', label: 'Ask', glyph: '?' },
  { id: 'voice', label: 'Voice', glyph: '◉' },
  { id: 'commands', label: 'Commands', glyph: '⌘' },
  { id: 'go', label: 'Go to', glyph: '→', children: [
    { id: 'go-runway', label: 'Runway', glyph: 'R' },
    { id: 'go-board', label: 'Board', glyph: 'B' },
    { id: 'go-floor', label: 'Floor', glyph: 'F' },
  ] },
  { id: 'flag', label: 'Flag it', glyph: '⚑' },
];

/* One assistant panel, two modes of each other: Ask (typed) and Voice (spoken).
   Either mode is one press from the other, in the head. */
function AssistantPanel({ mode, setMode, close }) {
  return (
    <>
      <div className="pane-head">
        <span className="pane-title">{mode === 'ask' ? 'Assistant' : 'Listening'}</span>
        <span className="modes" role="group" aria-label="Assistant mode">
          <button type="button" aria-pressed={mode === 'ask'} data-od-id="assistant-mode-ask" onClick={() => setMode('ask')}>? Ask</button>
          <button type="button" aria-pressed={mode === 'voice'} data-od-id="assistant-mode-voice" onClick={() => setMode('voice')}>◉ Voice</button>
        </span>
        <button className="overlay-close" aria-label="Close assistant" data-od-id="orb-panel-close" onClick={close}>×</button>
      </div>
      {mode === 'ask' ? (
        <div className="chat">
          <div className="msg">Three items need you. Want the P0 first?</div>
          <div className="msg me">Yes — what changed?</div>
          <div className="msg">service-auth#5 lost its second Keycloak node an hour ago.</div>
          <input placeholder="Ask the factory…" aria-label="Message" data-od-id="chat-input" />
        </div>
      ) : (
        <div className="voice">
          <div className="meter" aria-hidden="true"><i /><i /><i /><i /><i /></div>
          <div>"Show me what is blocked on go-live"</div>
          <button type="button" onClick={() => setMode('ask')}>Stop and type instead</button>
        </div>
      )}
    </>
  );
}

function Toggle({ value, options, onChange, name }) {
  return (
    <span className="group"><span>{name}</span>
      {options.map(o => <button key={o} type="button" aria-pressed={value === o} data-od-id={`lab-${name}-${o}`} onClick={() => onChange(o)}>{o}</button>)}
    </span>
  );
}

function Lab() {
  const [mode, setMode] = useState('radial');
  const [ring, setRing] = useState('floating');
  const [assistant, setAssistant] = useState('ask');
  const [panelOpen, setPanelOpen] = useState(false);
  const [theme, setTheme] = useState('dark');
  const [last, setLast] = useState('—');
  const stageRef = useRef(null);
  const [, bump] = useState(0);
  React.useLayoutEffect(() => { document.documentElement.setAttribute('data-theme', theme); }, [theme]);
  // The orb needs the stage element on first paint; re-render once the ref is set.
  React.useLayoutEffect(() => { bump(n => n + 1); }, []);
  // Ask and Voice are ring items that open the one assistant panel in that mode.
  const act = (id) => {
    setLast(id);
    if (id === 'ask' || id === 'voice') { setAssistant(id); setPanelOpen(true); }
  };
  return (
    <>
      <div className="labbar" data-od-id="labbar">
        <Toggle name="opens" value={mode} options={['radial', 'menu', 'panel']} onChange={setMode} />
        <Toggle name="ring" value={ring} options={['floating', 'plate']} onChange={setRing} />
        <Toggle name="theme" value={theme} options={['dark', 'light']} onChange={setTheme} />
        <span className="log">last action: <b data-od-id="lab-last">{last}</b></span>
      </div>
      <div className="stage" ref={stageRef} data-od-id="stage">
        {['Runway', 'Board', 'Floor', 'Attention', 'Timeline', 'Environments'].map(t => (
          <div className="card" key={t}><h3>{t}</h3><p>Stand-in content so the orb has something to float over.</p></div>
        ))}
        <div className="hint">drag the orb · flick it at an edge · press it to open · Ask or Voice open the assistant · arrows nudge it · Escape closes</div>
        {stageRef.current && (
          <FloatingOrb mode={mode} ring={ring} items={ITEMS} boundsRef={stageRef}
            onAction={act} panelOpen={panelOpen} onPanelOpenChange={setPanelOpen}
            renderPanel={(close) => <AssistantPanel mode={assistant} setMode={setAssistant} close={close} />} />
        )}
      </div>
    </>
  );
}


export { Lab };
