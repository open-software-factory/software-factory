import { defineConfig } from 'vite';
import { sharedPlugins } from './vite-plugins/shared.js';

/*
 * This config drives `vite` (the dev server). The dev server serves every
 * page from memory on request, so it needs no rollupOptions.input; visit
 * /shell.html or /orb.html directly. The production build is different —
 * see build.mjs — because vite-plugin-singlefile refuses more than one
 * HTML entry point in a single build.
 */
export default defineConfig({
  plugins: sharedPlugins(),
});
