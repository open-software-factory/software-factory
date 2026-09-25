#!/usr/bin/env node
/**
 * Build every page as one self-contained HTML file.
 *
 * vite-plugin-singlefile inlines a page's scripts and styles into its HTML,
 * but its own README says it does not support more than one HTML entry
 * point in a single build (multi-entry singlefile output is closed
 * "wontfix" upstream). So this runs `vite build` once per page instead of
 * once with rollupOptions.input listing all three, and only the first run
 * empties dist/.
 *
 *   node build.mjs
 */
import { build } from 'vite';
import { fileURLToPath, URL } from 'node:url';
import { sharedPlugins } from './vite-plugins/shared.js';

const root = fileURLToPath(new URL('.', import.meta.url));
const PAGES = ['index', 'shell', 'orb'];

for (const [i, page] of PAGES.entries()) {
  await build({
    root,
    plugins: sharedPlugins(),
    build: {
      outDir: 'dist',
      emptyOutDir: i === 0,
      rollupOptions: {
        input: fileURLToPath(new URL(`./${page}.html`, import.meta.url)),
      },
    },
  });
}
