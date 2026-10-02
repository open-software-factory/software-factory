/**
 * Public surface of the shell model.
 *
 * Deliberately absent: `./storage.browser.ts`. Each target binds its own
 * storage backend — memory for the design artifact, Web Storage in a browser,
 * the Tauri store on desktop — so importing this entry point never drags a
 * host API into a bundle that must not contain one.
 */

export * from './types.ts';
export * from './placement.ts';
export * from './layout.ts';
export * from './geometry.ts';
export * from './storage.ts';
export * from './persist.ts';
export * from './settings.ts';
export * from './surfaces.ts';
export * from './commands.ts';
export * from './inspect.ts';
export * from './interrupt.ts';
export * from './floatpos.ts';
export * from './views.ts';
export * from './popout.ts';
export * from './flick.ts';
export * from './radial.ts';
export * from './orbmenu.ts';
