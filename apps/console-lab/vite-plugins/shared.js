import react from '@vitejs/plugin-react';
import { viteSingleFile } from 'vite-plugin-singlefile';
import outputGuards from './output-guards.js';

/* Shared between vite.config.js (dev server) and build.mjs (the per-page
   production build). See build.mjs for why the build runs once per page. */
export function sharedPlugins() {
  return [react(), viteSingleFile(), outputGuards()];
}
