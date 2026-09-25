import React from 'react';
import ReactDOM from 'react-dom/client';
import { themeStylesheet } from '@open-software-factory/console-ui/tokens';
import '@open-software-factory/console-ui/overlay.css';
import '@open-software-factory/console-ui/orb.css';
import './styles.css';
import { Lab } from './Lab.jsx';
import './selftest.js';

const theme = document.createElement('style');
theme.setAttribute('data-od-theme', 'generated');
theme.textContent = themeStylesheet();
document.head.appendChild(theme);

ReactDOM.createRoot(document.getElementById('root')).render(<Lab />);
