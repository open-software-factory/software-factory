import React from 'react';
import ReactDOM from 'react-dom/client';
import { themeStylesheet } from '@open-software-factory/console-ui/tokens';
import '@open-software-factory/console-ui/overlay.css';
import '@open-software-factory/console-ui/orb.css';
import './styles.css';
import { App, store } from './App.jsx';
import './selftest.js';

const theme = document.createElement('style');
theme.setAttribute('data-od-theme', 'generated');
theme.textContent = themeStylesheet();
document.head.appendChild(theme);

/* Hydrate before first paint — the port is sync from here on, so the reducer
   never sees a promise. A real target awaits its store; memory resolves at once. */
/* An async boot turns a render error into an unhandled rejection, which no
   error listener sees. Catch it explicitly, or a broken build is a blank page. */
store.hydrate()
  .then(() => { ReactDOM.createRoot(document.getElementById('root')).render(<App />); })
  .catch(err => window.dispatchEvent(new ErrorEvent('error', { error: err, message: String(err) })));
