# Workspace spike: Dockview adoption findings

Spike file: `apps/console-lab/public/spikes/workspace-spike.html`.

The library is **dockview-core@8.2.0**, a framework-agnostic vanilla package. It is loaded from `https://esm.sh/dockview-core@8.2.0`.

There is no separate CSS import. The package's `package.json` has no `dist/styles/*.css` output. It marks `**/*.css` as a side effect. Styles are injected into `<head>` at runtime by the JS itself. This is also why it ships a `nonce`/`CspNonceProvider` option. Hosts with a strict Content Security Policy (CSP) hand it a nonce for that injected `<style>` tag.

This was verified against the package's published `package.json` and the `dockview-core` source on GitHub, under `mathuo/dockview` on the `master` branch. It was not verified from memory. The docs site (`dockview.dev`) is a client-rendered single-page app. A plain text fetch of it mostly returns navigation chrome. So the API names below are sourced from `packages/dockview-core/src/dockview/options.ts`, `theme.ts`, `theme.scss`, `dockviewGroupPanelModel.ts` and `api/component.api.ts`.

## Verdict table

| # | Requirement | Verdict | Evidence / adaptation |
|---|---|---|---|
| 1 | Theming | **PASS** | Dockview exposes every `--dv-*` variable. `theme.scss` defines 32 structural and color vars, and each one maps to a `--bg/--surface/--fg/--accent/...` token under a custom `.od-theme` class. The default render uses no built-in theme constant, such as `themeDark` or `themeAbyss`. Setting `tabGroupAccent: 'off'` turns off Dockview's built-in 9-swatch tab-color picker entirely. One of those 9 defaults is `--dv-tab-group-color-purple`. DESIGN.md §13.3 bans purple, so this is a real, needed adaptation. Control 1 in the spike swaps live to the unmodified `themeAbyss` constant and back. This lets the owner check the token-purity claim directly in the running page. |
| 2 | Layout: left, right, bottom, tabbed centre | **PASS** | `addPanel` with `position: { referencePanel: 'floor', direction: 'left'/'right'/'below' }` and `direction: 'within'` for centre tabs is exactly the documented shape. Confirmed against `AddPanelPositionOptions` in `options.ts`. |
| 3 | Interaction lock-down | **ADAPTABLE** | No single "lock" call disables tab drag, panel rearrange, and floating together while leaving resize untouched. Instead, three separate levers combine in the spike's `applyLockState()`. The spike calls `dockviewApi.updateOptions({ disableDnd: true, disableFloatingGroups: true, locked: true })`. It also sets `group.locked = 'no-drop-target'` on each group. This is the stricter of the two values on `DockviewGroupPanelLocked = boolean \| 'no-drop-target'`. Plain `true` still accepts non-center drops, confirmed in `dockviewGroupPanelModel.ts`. None of these three levers touch the gridview's sash and splitter drag path, so **resize keeps working**. This was verified by reading the drag-lock guard clauses, which gate only `position === 'center'` drop targets rather than sash pointer handlers. The top-level `locked` option's exact scope is undocumented on the docs site, a client-rendered SPA that is mostly unfetchable. It is applied belt-and-suspenders alongside the two options that are documented per field. This lock works at the group or component level. It controls drag, float, and resize together rather than gesture by gesture. |
| 4 | Overlay-to-pin lifecycle | **PASS** | Dockview has no "unpinned but visible" state. This matches `survey/02-docking-panels.md`'s original finding. The overlay is plain DOM outside Dockview entirely, using `#inspector-overlay`. It is absolute-positioned, with a 200ms `opacity`/`transform` fade and slide per §16.5 `--dur-panel`. "Pin" calls `dockviewApi.addPanel({ id: 'inspector', component: 'inspector', position: { referencePanel: 'floor', direction: 'right' } })`. "Unpin" calls `panel.api.close()`. Then the same DOM node, `inspectorContentEl`, returns to the overlay slot. It moves by reference via `appendChild`, keeping the original node instead of a copy. State and evidence rows persist across the transition, because the same DOM node and the same JS closures carry through it. |
| 5 | Animated transitions (rule 5.1) | **ADAPTABLE, fragile** | Treat this as unresolved until confirmed by manual testing. Reading `gridview.scss` confirmed that Dockview's base grid CSS has **no `transition` property anywhere**. Dock, undock, and focus-mode reflow are instant by default. This matches `survey/02-docking-panels.md`'s original finding that dock transitions are not eased. Sash-drag resize only *looks* smooth because it is a continuous pointer-driven update, rather than a CSS transition. Control 5 in the spike applies a blanket CSS transition, covering `left/top/width/height/flex-basis`, to `.dv-grid-view` and all its descendants as an experimental opt-in. This targets **undocumented internal DOM and class structure**, rather than a public Dockview contract. It may animate some reflows and skip others, depending on which CSS property Dockview's splitview engine writes on resize: an inline style or a class. A future Dockview patch release can change that internal structure without a semver-major bump, since the structure is internal. This could not be visually confirmed pixel by pixel in this environment, because no browser-automation tool was available here to capture and diff frames. The owner should open the file directly and toggle control 5 before relying on this. The overlay-to-pin motion, described in the overlay-to-pin lifecycle finding, is fully our own CSS and carries none of this risk. If the blanket-CSS hack proves unreliable in manual testing, the safer production path is the FLIP-transform technique, an animation pattern named First, Last, Invert, Play, already recommended in `survey/02-docking-panels.md`'s narrative. It applies around Dockview rather than inside it. |
| 6 | Focus mode | **PASS** | `dockviewApi.toJSON()` runs before collapsing. `panel.api.close()` runs on `nav`, `timeline`, and `inspector`, found via the confirmed `dockviewApi.panels` array. `dockviewApi.fromJSON(snapshot)` restores the layout. `toJSON` and `fromJSON` are Dockview's serialization signature feature. This is confirmed in `api/component.api.ts`, which defines `toJSON(): SerializedDockview` and `fromJSON(data, options?)`. They round-trip every group and panel size, so restored sizes are the same numbers that were serialized, rather than recomputed. Pixel-identical restore is a property of the mechanism itself. The `F` key is bound with a modifier guard, so it ignores the key when Ctrl, Alt, or Cmd is also held. Three 6px edge-affordance strips outside Dockview exit focus mode on click. They are plain DOM, and they belong to this spike rather than to Dockview. |
| 7 | Pop-out | **ADAPTABLE (feature-detected, no `window.open`)** | `'documentPictureInPicture' in window` gates the button. When the API is available, the spike calls `window.documentPictureInPicture.requestWindow({...})`. It then copies every same-origin stylesheet into the PiP (Picture-in-Picture) document. This means a `<link>` copied by `href`, or a cloned `<style>` built from `cssRules` for inline sheets. Cross-origin sheets are skipped in a `try/catch`, which is the correct, unavoidable behavior for that API. The spike **moves**, rather than clones, `inspectorContentEl` into that document via `appendChild`. So it is the live DOM node, carrying its live event listeners and evidence content rather than a snapshot. When the pop-out window fires `pagehide`, the node moves back to whichever container was current, either `overlaySlotEl` or the docked panel wrapper. Where the API is unsupported, the button is `disabled`. Its `title` names the production path, a Tauri `WebviewWindow` rendering the same panel from shared state. `window.open` is called nowhere in the file. |
| 8 | Persistence | **PASS** | `dockviewApi.onDidLayoutChange(() => persist())` writes `toJSON()` to `localStorage` on every structural change. This covers pin, unpin, resize, and close. On load, a `try/catch`ed `fromJSON` of the saved value runs before falling back to `buildDefaultLayout()`. The inspector's docked or overlay state is just "is there a panel with id `inspector` in the layout." So pin state persists for free as a side effect of layout persistence, with no separate flag needed. |

## Pop-out engine support matrix

This reflects browser and engine support as of September 4, 2026.

| Engine | Document PiP (`window.documentPictureInPicture`) | Source |
|---|---|---|
| Chromium, Google's open-source browser engine, used by Chrome and Edge | Supported since Chrome 116 | MDN, `Document_Picture-in-Picture_API` |
| Tauri's engine on Windows, WebView2 | Supported, because it tracks Chromium and Edge releases directly | Tauri webview-versions docs. Inherits Chromium's support |
| WKWebView and Safari, used on macOS and iOS. Tauri also uses this engine, Apple's WebKit, on macOS | **Not supported** | MDN compatibility notes. [The WebKit standards position on this](https://github.com/WebKit/standards-positions/issues/41) is still open and unresolved as of this writing. It is tagged with usability and portability concerns, with no committed position either way |
| Tauri's engine on Linux, webkit2gtk | Not supported, because it is built on the same engine as Safari, with the same gap | Same lineage as the Safari row in this table |

In practice, Tauri on Windows gets real picture-in-picture pop-out support. Tauri lacks this
support on macOS and Linux. That gap will persist until WebKit moves, and the open, unresolved
standards-positions issue gives no timeline for that. This is exactly why the pop-out
requirement asks for a feature-detected fallback, rather than treating Document PiP as the
only path. The Tauri `WebviewWindow` path, named in the disabled button's tooltip, is the one
that works on every OS Tauri ships to.

## Options and APIs used (exact names, source-verified)

- `createDockview(container, options)`
- `DockviewApi.addPanel`, `addGroup`, `toJSON`, `fromJSON`, `updateOptions`, `onDidLayoutChange`, `.panels`, `.groups`
- `DockviewOptions.theme`, `.disableDnd`, `.disableFloatingGroups`, `.locked`, `.tabGroupAccent`
- `DockviewGroupPanel.locked` (`boolean | 'no-drop-target'`)
- `AddPanelOptions.position`, using `referencePanel` plus `direction: 'left'|'right'|'below'|'within'`
- `IDockviewPanel.api.close()`
- `window.documentPictureInPicture.requestWindow()`

## Recommendation

**Adopt, with adaptations.** None of the eight requirements produced a hard FAIL. Two of them
carry real, stated risk rather than a clean pass. Interaction lock-down needs three combined
options, since no single "freeze everything but resize" switch exists. Animated dock and
undock reflow works only via an unsupported-internals CSS hack. That hack needs manual visual
confirmation before it ships, a risk stated in the animated-transitions requirement in the
verdict table.

Adopt Dockview for Workspace, as `components.md` already directs. Carry both adaptations
forward as named risks, stated openly rather than silently. Also keep the FLIP fallback from
`survey/02-docking-panels.md` on the shelf, as the fork-free escape hatch. Use it if the
animated-transitions CSS hack does not hold up under real use.

## Runtime verification corrections

These corrections were made on September 4, 2026, after the first real render. The verdicts
in the verdict table were written from source reading alone, before the spike was ever
rendered. The first real render showed a blank page. Four defects were found and fixed by
rendering the page repeatedly until it drew correctly. A proof screenshot, `verify-spike.png`,
confirms the fix.

1. **CSS claim corrected.** The earlier claim that Dockview needs no separate CSS import is
   wrong. Styles are not injected by the JS. `dockview-core@8.2.0` ships zero CSS. The
   structural stylesheet ships in the framework packages instead. The spike now links
   `dockview-react@8.2.0/dist/styles/dockview.css`, verified live on jsDelivr. Without it,
   panels render as an unpositioned stack.
2. **`createDockview()` returns the API directly.** In Dockview 8.x, it returns the
   `DockviewApi` object itself, rather than a component wrapper with an `.api` property. The
   spike code assumed `.api` existed, and it died on `addPanel`.
3. **`localStorage` throws in this preview.** The page is served from a `data:` URL, and even
   reading `localStorage` raises `SecurityError` there. The unguarded read killed init after
   the dock mounted but before panels were added. This was the original "nothing shows"
   symptom. All storage access is now behind a guard. Persistence degrades visibly instead of
   failing silently.
4. **Sizing needed an explicit pass.** This meant calling `api.layout(w, h, true)` and
   `setSize` after build. Without it, the nav group took nearly the full width in the first
   paint.

This is now standing policy. A spike is not "done" until someone renders it and looks at the
result. Syntax checks and source reading are not evidence of a working page. The spike now
also surfaces every failure visibly, since global error handlers paint the error into the
page, instead of dying silently.
