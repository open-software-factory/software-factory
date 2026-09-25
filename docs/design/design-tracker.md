# Design tracker

Working state for the operator-console design effort. Three documents divide the work. `DESIGN.md` holds guidance, in the form of decisions and tagged candidates. `design-explorations.md` holds discarded directions. This file holds open items, next steps, and review fixes in progress.

## Current checkpoint, `2026-09-11`

`apps/console-lab/shell.html` is the base prototype page. React, a UI library, and TypeScript, a typed variant of JavaScript, are the chosen frontend stack. Tauri, a desktop app framework, is the chosen desktop runtime. Both shell themes are built from `packages/console-ui/src/tokens.ts`. Motion is always on. There is no motion setting and no host-preference gate. Review entries below are history. They are not instructions to restore superseded controls.

The Move menu was deliberately removed as the wrong affordance. `api.moveSurface` remains as the tested placement entry point for future drag-and-drop. That decision is preserved. The existing narrow navigation drawer is the first shared Overlay consumer.

**Landed:** `packages/console-ui/src/overlay.ts` and `overlay.css` are bundled into the shell. They are composed from React Aria Components `1.21.1`, a library of accessible UI primitives. The parts used are Modal, Dialog, and ModalOverlay, the outer wrapper that supplies the overlay layer. The drawer has modal semantics. It has focus containment, focus return, outside mouse or touch dismissal, and Escape dismissal. Entry and exit use token-driven WAAPI animation. It stays mounted and modal until exit completes. Both view selections and the close control use that lifecycle. Returning to a wide viewport closes the drawer. It also restores focus to an available shell control if the original trigger disappeared. Panel switches keep the drawer open, as before, and placement logic is unchanged.

**Verification:** 13 DOM interaction tests exercise the real component, built on React Aria, a library of accessible UI primitives, and a controlled animation clock. The rendered suite J confirmed four distinct positions on entry and exit, focus capture, and stage fit. Its layout, final-removal, and focus-return assertions were sampled too early. The harness now waits for stable layout and animation completion. Those revised rendered assertions have not been re-exported. Focus return, dismissal, and exit lifetime pass the DOM tests. The DOM clock proves lifecycle timing only. A rendered capture is still needed to confirm visual interpolation. The diagnostic capture is named verify-overlay.png. It is kept in the design workspace. It is not a file tracked in the repository. It still shows the initial timing failures. All self-tests are off in the delivered page.

**Landed `2026-09-11`, command palette** (`packages/console-model/src/commands.ts`, `packages/console-ui/src/palette.ts`). This is the second Overlay consumer, at a new `top` placement. Ctrl+P, the palette shortcut, or Cmd+K, its Mac equivalent, opens it from anywhere. The top-bar search control, now a real button, also opens it. Escape, an outside click, or running a command closes it through the shared lifecycle. Focus returns to wherever the operator was.

Commands are *intents* derived from the placement model, so the list follows the navigation structure. Surfaces in the centre are views. The rest are panels with open and close verbs. Every accepted slot yields a "Move X to Y" or "Open X in Y" entry. This is the keyboard twin of the retired Move control. It is listed only once there is a query.

Work items come from the board rows. Runs come from the log fixture. Evidence has no fixture, so it is honestly absent from the list. Picking a work item opens the inspector, docked, on that item. DESIGN.md, the guidance document, sets this operator-requested pattern at §5.1.1. The inspector shows the header reference plus the board row's column, labels, and state, and it invents nothing.

Filtering is ranked in this order: label prefix, then label word, then label, then hint or keyword. Every token must match, and groups never split. React Aria's Autocomplete and ListBox parts, the list-selection primitives, supply the virtual cursor. Typing focuses the first match. Arrow keys move the cursor. Enter runs the focused command. Focus never leaves the field.

**Verification:** 15 model tests (`commands.test.ts`) and 10 DOM tests (`palette.test.ts`) run on the real React Aria components. Browser suite K drives the shipped page. It checks the shortcut, the ranking, the first-match cursor, Enter selecting Runway in the centre, close-and-return, a blank reopen, and `#87` opening the inspector. Its rendered capture is named verify-palette.png.

Two traps were found only by rendering. First, the bundle threw an error at load. A CommonJS `require("react")` call survived bundling, from use-sync-external-store, behind React Stately, a React Aria state library. Vite now bundles React itself, and the output guard in `apps/console-lab/vite-plugins/output-guards.js` fails the build if a `require()` call survives. Second, React Aria triggers a row on the keydown and keyup pair together, so a keydown-only synthetic Enter does nothing.

OpenDesign, the host application that renders this design workspace, also exports `NODE_ENV=production`. This silently breaks React's `act()` testing helper, and `packages/console-ui/test/dom-env.ts` resets it.

**Owner review `2026-09-11`.** Controls read as square-cornered, and the palette field wore the dialog-wide accent ring. `--r-control` is now `2px`, the value DESIGN.md §13 always specified. A `1.5px` value rendered as no corner at all. The palette field indicates focus with its own accent rule and glyph instead of a ring. The virtual-cursor row carries the fill alone. The resting capture is named verify-palette-rest.png, from the look-only suite P.

**Landed `2026-09-11`: the inspector.** It supports select, float, and pin or unpin. That lives in `packages/console-model/src/inspect.ts`, with the non-modal mode in `packages/console-ui/src/overlay.ts`.

Board cards, attention items, and floor nodes are selectable. They respond to click, Enter, or Space. A hairline marks the selected one.

The form follows the trigger. This is exactly how DESIGN.md §5.1.1 was amended. Content selection floats the inspector as a non-modal Overlay at the right edge. There is no scrim and no focus capture. The content underneath stays live. The next click just swaps the subject. The panes do not move.

Pin docks the inspector through the placement reducer. The docked pane then offers Float, which lifts it back out with its subject. A palette pick is operator-requested and docks directly. Escape closes the float only from inside it. "Docked" is never stored as its own state. It simply means the inspector surface is visible in a pane.

The inspector body shows what the source list knows, board column and labels, attention age and detail, and floor priority, and nothing is invented.

**Two reducer gaps.** The browser suite found both, and both are now fixed with tests.

The right region toggle *hides* the pane, but it leaves the surface placed. This created two problems. First, "docked" now means visible. Placed alone was not enough, because a look would otherwise activate an invisible pane. Second, `open` on a surface already sitting in a hidden region was only an activation. The reveal now applies there too, so pin always shows something.

**Verification:** 13 model tests in `inspect.test.ts`, including the hidden-region cases, 3 non-modal DOM tests in `overlay.test.ts`, and one reducer test for the reveal.

Browser suites L and M split the work so each fits the capture window. Suite L covers float without layout change, focus not stolen, and subject swap. It also covers pin docking and revealing the hidden region together, the docked header, and the Float control. Its capture is named verify-inspector.png. Suite M covers a docked click swapping the subject, unpin floating the pane, and floor node selection. It also covers Escape inside closing the float and handing focus back. Its capture is named verify-inspector-unpin.png, and all 17 rows pass.

A test-harness note: `assert.equal` on two DOM nodes stalls Node's assert on serialisation when it fails. Focus checks now compare with `===` instead.

**Owner review `2026-09-11`: shell chrome.** The centre pane no longer carries a close control at the edge of its tab strip. `Pane` takes a `closable` property, defaulting off for the centre, since a view is left through the nav or its tab instead.

The region toggles used to name a fixed surface label, and their pressed state ignored visibility. They now read Hide or Show, and they name what the region holds by subject, for example `Hide service-auth#5` or `Hide service-auth#5, run-4471`. They are pressed only while the region is actually showing.

Suite I gained three rows. Its capture is named verify-suiteI.png. The last two rows run past the capture window. They are covered instead by the reducer's toggle tests.

Planning stays in this file. The short-lived `next-steps.md` file was folded into the "Next steps" section of this document and removed.

**Polish `2026-09-11`.** Pane seams are `7px` in both directions. Grips overlay the seam instead of adding to it. Suite X prints the measured gaps. Its capture is named verify-gaps.png.

**Landed `2026-09-11`, A2: P0 decision dialog** (`packages/console-model/src/interrupt.ts`, fixture `data/p0.json`).

This is a modal `alertdialog` on the shared Overlay, at a wide centre placement. It is not dismissable. Escape and the scrim do nothing, because leaving is not a decision.

The pause is a fact the dialog reports. Minimise swaps the dialog for a persistent critical strip under the top bar. That strip is never an overlay, and the strip itself reopens the dialog. Three operations, expand containment, run the playbook, and abort, confirm on the same control. Each is audited in the interruption's log, and each leaves the pause in force. Only Resume, with a recorded justification, resolves the pause. An empty or whitespace justification is refused, and the field is flagged.

The lab bar can also raise a P0, since a system-raised event has no product control of its own. The content is the floor's P0 scenario, carried over as fixture data.

Verification: 8 model tests (`interrupt.test.ts`). Browser suites Q and R give rendered evidence for the alertdialog role and focus capture. They also cover Escape and scrim refusal, minimise leading to the strip and back to reopen, and the flag clearing on input. Their captures are named verify-p0.png and verify-p0-resume.png. The abort confirm and the final resume ran past the capture window. They rest on the model tests instead.

**Landed `2026-09-11`, A1: Popover** (`packages/console-ui/src/popover.ts`).

There are two anchored contracts built on React Aria. `Popover` is a small labelled dialog beside its trigger. `ChoiceMenu` offers a single choice, marks the current one, and closes on picking.

React Aria positions the popover and flips it near edges. It owns focus, Escape, and outside dismissal. Motion is CSS, driven by its `data-entering` and `data-exiting` attributes with the shared duration and easing tokens, and it waits for the exit to finish.

The first consumers are in the top bar. The environment pill is now a real button that opens a scope menu: prod, staging, or dev, read from `data/environments.json`. That menu is a scope selector only. Nothing filters by it yet. The "need you" pill opens the list of items that need the operator. Each row selects that item, so the inspector shows it.

Verification: 5 DOM tests in `popover.test.ts`, covering open and focus, Escape and focus return, outside click, choice marking and close, and keyboard use. Browser suite S shows the menu anchored below its trigger and the choice updating the pill. It also shows the list opening as a dialog with 3 rows, and a row selecting into the docked inspector. Its capture is named verify-popover.png.

A harness note: React Aria names a menu after its trigger. It also runs `instanceof` checks against more DOM constructors than the test environment had exposed. `dom-env.ts` now exposes them all.

**Landed `2026-09-11`, A14: floating inspector polish** (`packages/console-model/src/floatpos.ts`).

The float drags by its header, and it is clamped to the stage. It remembers where it was left, as an offset from its anchored home, `dx <= 0` for left and `dy >= 0` for down. That offset persists through the storage port, under its own key. This offset is layout state that belongs to the float alone, and it is not read as a general preference.

It is now content-tall instead of stage-tall. It had been stretching to the full height, which also left it no room to move vertically.

At the narrow breakpoint it becomes a full-width bottom sheet with no drag handle.

A second model gap surfaced here. At that breakpoint the side panes have zero width, so a "docked" inspector would be invisible there. `isDocked`, `inspectMode`, and `inspectPlan` now account for the slots the viewport cannot show. A selection floats instead, rather than swapping the subject of a pane the operator cannot see.

Verification: 7 model tests in `floatpos.test.ts`, and one more in `inspect.test.ts`. Browser suite T covers the drag handle, the grabbing state, and pointer movement in both axes. Its capture is named verify-float-drag.png, and it also shows the clamp holding the float at the stage's top-left. Browser suite V covers the narrow case: no side width, a sheet full width at the bottom edge, no handle, and the subject shown. Its capture is named verify-float-sheet.png. Suite U checks that position is remembered across close and reopen. It runs past the capture window, so that behaviour rests on the model's round-trip test and on the state living outside the overlay.

This also fixed top-bar pills that wrapped at narrow widths.

**Landed, A6.** Keyboard, touch, and target audit, done `2026-09-11`. Every interactive surface in the shell was checked against DESIGN.md §5.4.1 and §5.4.3. What was cheap to fix was fixed. What was not is listed below.

| Surface | Found | Now |
|---|---|---|
| Tab strips (centre, side panes) | Every tab a tab stop, no arrow keys | `role=tablist` with roving focus, one stop per strip. Left, right, Home, and End move and activate |
| Pane grips | Pointer only, hover-revealed | Focusable `role=separator` with a label. Left and right resize the side panes, and up and down resize the bottom one, in `16px` steps, through a new relative `resizeBy` reducer action, so key repeat faster than a render still adds up. The handle shows accent on focus. The nav-width grip toggles rail and wide the same way |
| Views | No shortcut | Alt+1, Alt+2, or Alt+3 jump to the views in nav order (DESIGN.md §16.3) |
| Icon-only controls | `22px` to `26px` | Unchanged for mouse. `(pointer: coarse)` raises pane actions, region toggles, tabs, nav rows, and grip hit zones to `44px` |
| Top bar at narrow widths | Pills wrapped to two lines, brand text crowded the search | Pills never wrap. At the `sm` breakpoint the brand text hides, the search shrinks to fit, and the ends keep one row |
| Focus rings | Global `:focus-visible` on native controls. React Aria controls carry `data-focus-visible` styles: pills, palette rows, menu items, cards, and nodes | Verified present on every control type touched this session |
| Drawer, palette, popovers, P0 dialog, float | Escape and focus return already covered by DOM tests | no change needed |

Still open under DESIGN.md §5.4. The rail's icon buttons rely on `title` for their name. That is acceptable, but `aria-label` would be cleaner. Pane tab close (×) is a span inside the tab, and it is not separately focusable. A keyboard user closes a view from the nav or the palette instead. The region toggles' pressed state is now correct, but they have no visible label beyond the tooltip.

**Verification.** A reducer test covers `resizeBy`. Browser suite W checks the tablist and a single tab stop. ArrowRight, the key, activates and focuses the next tab. It also checks Alt+3, the separator role, and two arrow steps equalling `32px`. It checks a narrow top bar staying on one row and pills staying on one line. Its capture is named verify-keyboard.png, and all 8 rows pass.

**Landed `2026-09-11`, A4: per-view layout** (`packages/console-model/src/views.ts`).

The workspace wraps the layout with a memory of each view's side and bottom panes. Navigating away stores them. Navigating back restores them. A first visit keeps whatever is open.

Only navigation switches this memory. That means opening, activating, or closing something in the centre. A move, or a focus-mode restore, changes the layout on purpose, and that choice is never second-guessed. A recalled pane cannot bring back a surface that has since moved into the centre.

The memory persists inside the existing layout payload, as an optional `views` field. Old payloads still load. Unknown views and unplaceable panels are dropped on the way in.

**Verification.** 9 model tests in `views.test.ts`, including the persistence round trip. Browser suite Y shows a first visit keeping the inspector, and Board opening run output. It also shows Floor never having had it, and Board getting it back with run output active. Its capture is named verify-per-view.png. The last two rows run past the capture window and rest on the model tests instead.

**Landed `2026-09-11`, A3: drag-rearrange.** Hold a tab or a pane header for `6px` and it becomes a drag. The ghost carries the surface's name. Every region shows a zone. The surface's own region reads "here", and a region that does not accept it is dimmed. The pointed zone lights up. Dropping calls the reducer's `move` action. The view decides where the pointer is. The reducer decides what a drop means and which slots the surface accepts.

Escape cancels. Hidden regions still offer a zone of at least `160px`, so a panel can be dragged into an empty side. This is off at the narrow breakpoint, where the side regions have no width.

**Verification.** Browser suite Z checks that a nudge is not a drag. It checks that there are four zones, and that the pointed zone lights up. It checks that the surface's own region reads "here" and is not a target, and that the ghost carries a label. It also checks that a drop moves Runway to the right and makes it the active tab there, and that Escape cancels. Its capture is named verify-drag.png, and it shows the zones in flight. No new model was needed, because `move` and `acceptedSlots` already carry the rules. The palette's Move commands remain the keyboard route.

**Owner decision `2026-09-12`.** Widths are global. Presence is per view. Item A4 had remembered each view's side panes, including their widths, so a seam moved on every view switch.

A resize states how much room the operator wants on this screen. It is not something about the view itself. A jumping seam reads as the layout shoving itself, against the spirit of DESIGN.md §5.1.1.

Per-view memory now carries which panels are open and which is active. Region widths and navigation width stay wherever the operator last put them. A remembered panel still reveals a region the operator had hidden.

`views.ts` recall was rewritten. 3 tests were added. They cover resize carrying across views, nav width being global, and reveal happening on recall. DESIGN.md §4.2, §5.2.3, and the catalogue's Workspace row said "per-view sizes", and all three were amended.

One case might still want a per-view width: a dense table beside a narrow inspector, or a canvas beside a wide one. That case is left to the surface's width appetite when it appears, rather than to memory.

**Landed `2026-09-11`, A8: breadcrumb.** The inspector shows where its item came from, as a path, for example `Board > Backlog > service-portfolio-api#87`, `Attention > Ready > all-about-money-ui#319`, or `Floor > ...`.

Each level is a one-click jump back. A view opens in the centre, and a panel opens in its home slot. The current level is shown as plain text instead of a link.

Only real jumps are offered. Clicking a board column in the path opens the Board view, since column-level focus does not exist yet.

**Verification.** Browser suite AA shows the path with source and column, and confirms the current level is not a link. Its capture is named verify-crumbs.png. The jump-back and the attention path rows ran past the capture window. They rest on the same `openCentre` and `open` functions the nav uses.

**Landed `2026-09-11`, A5: pop-out as a port** (`packages/console-model/src/popout.ts`).

The shell asks a `PopOutPort` for two verbs and a flag. It never calls `window.open` directly. A test asserts that the source code does not mention it.

The design artifact binds `noPopOut`. Every side and bottom pane then carries the pop-out control, visibly disabled, with the reason "Own window needs the desktop app". This is the same stance taken for a refused slot.

The Tauri adapter, `WebviewWindow`, is the app package's job. It remains **unverified**. Nothing here proves that a window actually opens.

3 model tests cover this.

**Owner review `2026-09-12`: polish pass.**

| Finding | Now |
|---|---|
| Bottom region toggle never read as pressed | A reducer gap. Opening into the bottom left its mode `hidden`, so "shown" was never true. Fixed in `addTo` with a test. The toggle follows |
| Floor background differed from Board and Runway | The Floor sits on the pane surface like every view. The lit and vignette depth tokens are parked until the centre is designed as loop views (A10), where one decision covers all centre content |
| Accent shown as coloured edges, such as card left borders, nav marker bars, and attention inset bars, read as machine-generated styling | No edges anywhere. Cards carry a state dot beside the reference. The current view is a chip behind it, plus an accent glyph. "Needs you" is the attention halo on the state dot. The halo means attention only, per the §11 grammar |
| P0 strip and dialog header colour | Neutral raised surface (`surface-2`). Only the severity tag and a `30%` hairline are red |
| Strip "Open" and dialog "Minimise" were word buttons | Icon buttons with tooltips. This is the pattern for layout controls from here on |
| Scrollbars always visible | Shown only while the pointer is over the scrolling area. Browsers cannot fade native scrollbars, because the properties do not animate, so this switches instantly instead of fading. A custom overlay scrollbar would be needed for a fading bar. That is parked for later |
| Selection changes snapped | A single highlight per rail, nav list, and tab strip glides to the current item, using `useSlidingHighlight`, WAAPI like the pane sizes. A CSS transition proved unreliable for whichever highlight moved second |

**Verification.** A reducer test covers the bottom reveal. Browser suite AB checks many things. These include the bottom toggle's pressed and unpressed states, tab and rail highlights sampled at intermediate positions, and the absence of marker bars. They also include cards using a dot instead of an edge, the halo on the dot, and icon buttons on the strip and dialog. They include a neutral strip and the Floor without a gradient. Its capture is named verify-polish.png. The capture window catches only the first rows. The rest passed in an earlier full-length capture of the same suite.

### A12 plan: the ambient orb (proposed `2026-09-12`)

Today there is a v1 draggable orb in `apps/console-lab/shell.html`, with a fixed menu.

The contract, DESIGN.md §5.3, asks for more from two components: L1 `FloatingOrb`, the draggable trigger, and L3 `AmbientAssistant`, the assistant surface it opens. It wants a circular draggable button above everything. That button opens a menu shaped by *context and viewport*. It is a cross-cutting modality, one of command, context action, annotation, or generated view. It is neither a surface nor a chat tab. Question Q12, which modality matters first, is still open.

Proposed slice, in order:
1. **FloatingOrb**, the draggable trigger, on the shell. It drags like the inspector float, reusing `floatpos.ts` with its own key. It snaps to the nearest edge on release. It stays above overlays but below the P0 dialog. It has a `44px` target and is keyboard-reachable, with a shortcut and Tab order.
2. **Context menu on the Popover primitive.** The menu is built from context, the same way palette commands are: current view, selected item, active pane, expressed as *intents*. Example intents: "Explain this item", "Summarise the Board", "What changed since 14:58", "Annotate", plus the palette's actions. The same model discipline applies: an `assistantMenu(context)` function with tests.
3. **Input.** A TextInput, the text-entry primitive, in the popover takes a free question. Voice, item A11, plugs in here later. There is no transport yet, per Q12. A typed question yields an honest "no assistant connected" state instead of a fake answer.
4. **Generated view.** This is parked with A13 and StreamSurface, the generated-UI surface. The orb needs the part schema first.

The owner still must answer Q12. That question asks which of the four modalities matter first, and whether the orb may dock into a pane or stay floating only.

**Landed `2026-09-12`, orb lab.** This spans `packages/console-ui/src/orb.ts` and `orb.css`, `packages/console-model/src/flick.ts`, `packages/console-model/src/radial.ts`, and `apps/console-lab/orb.html`.

The owner's direction was to start the orb as a React component in its own file, untangled from the shell. This came after a look at what already exists.

The survey, `survey/07-floating-orb.md`, found this. No shipped component models a draggable trigger with a radial menu. The radial-menu packages are mouse-first, with no keyboard or ARIA story. One drags in a second icon library. One is abandoned. Motion for React would give drag and momentum, but its boundary bounce is a spring rather than a reflection. It is also a large dependency for one gesture.

The verdict: adopt nothing. Compose the orb from what is already in the bundle.

The lab's `FloatingOrb` knows nothing about placement or surfaces. The host gives it items and a bounds element, and gets an id back.

Press, under `6px` of movement, opens the orb. Drag moves it, clamped to the bounds. Release while moving flicks it. Momentum decays, and walls reflect the normal component while keeping the tangential one, scaled by a restitution. This means it leaves a wall at the angle it arrived. A pause before release is a drop instead of a flick, because the release itself is the last velocity sample.

The lab bar switches between three things the orb can open. **Radial** is a React Aria `Menu` laid out on a ring. It has roles, Home and End, typeahead, and Escape from the menu, and all four arrow keys walk the ring. Submenus form a second ring with a Back item, with items radiating out along their spokes. **Menu** shows the same items in a React Aria `Popover` beside the orb, opening up or down depending on where the orb sits. **Panel** is the shared non-modal Overlay parked beside the orb. It holds whatever the host draws, such as a mini chat and a voice panel in the lab.

In a corner or at an edge, the ring opens only into the arc that fits. The radius then grows so items on a short arc do not overlap. This is handled by `ringLayout`, which is tested. The orb sits above everything, with a drop shadow, and sinks when pressed. Arrow keys nudge it from the keyboard.

**Verification:** 7 physics tests in `flick.test.ts`. They cover release velocity from the last stretch, decay to rest, reflection keeping the angle, and a corner shot never leaving the field. 5 layout tests are in `radial.test.ts`.

Lab suites drive the real component with synthetic pointers:
- ring, 13 rows
- drag, 5 rows
- flick, 4 rows
- variants, 7 rows

Their captures are named verify-orb-ring.png, verify-orb-drag.png, verify-orb-flick.png, and verify-orb-variants.png. Look-only captures add verify-orb-show-ring.png (a corner case), verify-orb-show-centre.png, verify-orb-show-edge.png, and verify-orb-show-menu.png.

A trap was found only while rendering. The exporter shrinks its viewport during boot, as F22 recorded. Because of this, a synthetic "move then release" read as a flick, until a release sample was added. Every measurement then had to be taken fresh.

**Owner direction `2026-09-12`, applied.** The press now opens the **radial menu** only. The regular menu and the panel stay as patterns reached from ring items instead of direct presses.

**Ask** opens the assistant panel in typed mode. **Voice** opens it in listening mode. These are one panel with two modes of each other, and each mode is one press from the other, in the panel head. This is `AssistantPanel` in the lab. The orb gained `panelOpen` and `onPanelOpenChange` props, so a host can open the panel from anywhere.

Items are now **born in the orb**. Each starts at the centre, small and faint, and travels out along its spoke with a touch of overshoot, staggered by `28ms`. The orb itself squeezes and releases as it gives birth. React Aria mounts menu items in a second render pass, so the animation starts from each item's ref callback, once per element. A layout effect on the menu found no items on first open, and only animated on re-layout.

A new **plate** variant was added, set with `ring="plate"` and the lab bar's "ring: floating or plate" control. An opaque disc or sector comes from `platePath`, based on the arc that `ringLayout` now reports. It grows from the centre under the items and covers what is underneath. The orb stays on top of it. The orb layer clips at its bounds, so a plate reaching past a corner cannot make the host scroll.

Lab suites born (6 rows) and assistant (7 rows) were added. Their captures are named verify-orb-born.png, verify-orb-assistant.png, verify-orb-show-plate.png, and verify-orb-show-plate-centre.png.

**Owner review `2026-09-12`, applied.** Both ring looks stay, chosen by the `ring` prop, either `floating` or `plate`. The default is decided later.

Three fixes were made.

First, the corner arc now covers the whole quadrant. Labels moved inside the item disc: a glyph over a `9px` label, with an ellipsis past `48px`. Because of this, an item needs only its own half-size of clearance, and the end items can sit level with the orb. For five items, the corner radius fell from `254px` to `199px`.

Second, the plate label clipping had the same cause. A label hanging below an item at the arc's end had fallen past the stage edge. Moving labels inside fixed this. The plate sector also now runs 12 degrees past its end items. At a wall it therefore meets the edge and is clipped there, instead of leaving a sliver.

Third, the "dot" in Go to was the submenu chevron, drawn loose in the disc. It is now a `16px` badge at the item's shoulder.

Captures were refreshed, covering verify-orb-show-plate.png, verify-orb-show-ring.png, verify-orb-show-plate-centre.png, and verify-orb-show-edge.png. Lab suites ring, born, assistant, and variants all pass.

**Accepted `2026-09-13` as orb v1**, tag `orb-v1`, commit `53a85fc`. The owner reviewed the lab in a real browser and took this as the first version of the orb for the app.

The next step is wiring it into `apps/console-lab/shell.html`, covered in the entry headed "the orb in the shell". The open questions listed under "To judge in the shell now" stay open and can be settled in the shell.

**Landed `2026-09-13`: the orb in the shell** (`packages/console-model/src/orbmenu.ts`, `apps/console-lab/shell.html`).

The orb sits above the panes and floats, and below the P0 dialog. Its layer is `945`. That sits between the float layer, `940`, and the modal layer, `950`.

Its place is an offset from its home corner, kept under its own storage key, `ORB_KEY`. `floatpos.ts` takes a key for this. A resize keeps the offset rather than the coordinates, so a corner orb stays in the corner. The exporter's boot-time viewport growth exposed that need.

The ring is built from the same model the palette uses, `orbMenu`, which is tested. It offers Ask, Voice, Commands, and **Go to**. Go to lists the views in nav order, with the current one marked by the accent, and **Hide nav** at the end. It also offers a context submenu named after the selection: Inspect *ref*, Ask about *ref*, and Ask about *view*.

Two of the owner's explorations are now in the shell.

First, **the orb as the collapsed nav**. When the left navigation is hidden, either in focus mode or by Hide nav from the ring, the views sit on the ring itself. A **Panels** submenu carries the panel toggles and **Show nav**. Navigation is otherwise never taken away, since the region toggle rule still stands.

Second, **context as intents**. The selection and the current view become ring entries, using the same intent data the palette runs, so nothing on the ring is a closure.

Ask and Voice open one **assistant panel**, the inspector's non-modal float, in two modes, with a switch in the head. Nothing is connected behind it, and the panel says so plainly. A typed question is kept but never answered by a fake response.

**Verification:** 6 model tests in `orbmenu.test.ts`, plus 1 more for the keyed offset in `floatpos.test.ts`. Shell suites cover 25 rows, all passing:
- AC: the ring follows the nav, the current view is marked, it navigates, and focus returns.
- AD: the selection names the context and intents, and Ask about opens the assistant with the subject. The empty state is honest, and voice is one press away.
- AE: Hide nav makes the orb the nav, and it navigates without a rail.
- AG: the Panels submenu, and Show nav restoring the rail.
- AF: place is remembered through the port, and a narrower stage keeps the offset from the corner.
- AH: Inspect from the ring docks the item, and focus goes back to the orb.

Their captures are named `verify-orb-shell-*.png`, and the look capture is `verify-orb-shell.png` from suite O, with Node tests totalling 219.

**Owner review `2026-09-13` (applied).** Hide nav does not belong on the ring. Hiding the navigation is a mode with its own visible exit, so it should not be a menu entry. It was removed, along with Show nav. The orb takes over navigation where the nav is not on screen, in **focus mode** and at the **smallest size**. There, the nav is a drawer (`navHidden = left.mode === 'hidden' || bp === 'sm'`). Suites AE and AG now enter focus mode from the topbar control and leave it with Esc. AG also checks the 820 px viewport. 6 model tests cover this, including "the ring never offers to hide or show the nav".

**To judge in the shell now:**
- The context submenu is named after the selection ref (`all-about-money-ui#319`). The title ("Prod Wiredash credentials are invalid (401)…") is friendlier but long. In a 56 px disc, the 9 px label shows about eight characters either way. The real choice is therefore a short ring label ("This item"), with the full ref or title as the accessible name and tooltip.
- Floating or plate as the default.
- Whether that choice follows the position, plate in a corner and floating in the open, or is one setting.
- Whether the in-disc labels read well enough at 9 px, or the ring should show labels only on hover and focus.
- Birth motion tuning, to feel in a real browser:
  - 280 ms per item
  - 28 ms stagger
  - 4% overshoot
- Flick tuning, to feel in a real browser (the exporter cannot):
  - friction 0.004/ms
  - restitution 0.55
  - release threshold 0.25 px/ms
- Snap-to-edge on release, which the plan above recommended, versus stay-where-dropped, which is what the lab does today. Both are one line in the release handler.
- Whether the ring's labels should show at all, or only on hover/focus.
- Q12 still stands: which modalities matter first, and whether it should float only or dock into a pane like the inspector.

**Next:** react to the lab. Then wire the chosen form into the shell, with its position remembered through `floatpos.ts` under its own key, above overlays and below the P0 dialog. Build the context menu as intents, using `assistantMenu(context)`, tested. Call A16 when this is done.

### UI scale 125 % (owner request `2026-09-15`)

The owner found the console easier to read at 125% browser zoom, and asked for all sizes to go up 25%, layout included. This is a real scale rather than a zoom. Every size in the shell's stylesheet, `overlay.css` and `orb.css`, is now **rem**. Hairlines under 3 px, borders, outlines, shadows, and media queries stay in px. The root font-size is **20 px**, and that one number sets text, spacing, controls, seams, and the orb together, per DESIGN.md §12.2 "UI scale".

The placement model's widths carry the same scale explicitly, in `geometry.ts`:
- rail 90
- compact rail 60
- navigation 310
- seam 9
- centre floor 700

Pane appetites scale by ×1.25 in `surfaces.ts`. The start layout is 360 by 430.

The orb's JS geometry, its disc, size, ring radius, and panel width, now follows the root font-size. So the lab at 16 px is unchanged, and the shell gets the scaled orb. CSS `zoom` was rejected, because it would have scaled every pointer delta, resize, drag, and flick, by 1.25 against the model.

On the way, this surfaced a gap: the centre floor rule, `fitToViewport`, tested since the geometry work, had never been applied by the view. At the old scale the centre always had room, so the gap stayed hidden. At 125%, on a 1440 stage:
- the centre had fallen to 540 px
- the floor graph shrank with it

The view now fits the side panes to the stage instead, so the centre keeps 700 px. At 1440 px the sides now give way in proportion:
- L 382, R 358 today
- down from L 459, R 430

**Verification:** 220 Node tests cover this, with geometry test sizes moved inside the new appetites. Shell suites AC and AF pass at the new scale. Captures include:
- `verify-scale.png` (rest)
- `verify-scale-nav.png` (navigation expanded)
- `verify-scale-palette.png`
- `verify-scale-orb.png`
- `verify-scale-gaps.png`, from suite X, showing seams of 9 px and 4.5 px

Two rows of the old suite A, "tabs only when >1" and "centre tab animates", still fail after this change. They have been stale since the tab strip redesign, a known issue rather than a new regression, and they are noted here without being fixed.

### Q20: where the design work lives now that the factory repo is public (opened `2026-09-21`)

The product repo is `open-software-factory/software-factory`. It has a Rust engine, decision records, UX docs, and a project board with Workstream, Evidence, Priority, and Size fields. Strict prose linting runs on every document. Its console-stack record says the two prototype packages move in unchanged, with Vite as the bundler.

The owner decided the following on 25 September 2026.

1. **Home for artifacts.** The design work lives in this repo. A separate console repo was rejected.
2. **Prose gate.** The design documents pass the prose linter with no exemptions. They were rewritten to pass.
3. **Layout.** The repo is a monorepo for the engine, apps and shared packages. The target shape is under "Repository shape" below.
4. **Package manager.** JavaScript uses one pnpm workspace. Many git worktrees of the repo exist at once, and pnpm links packages from one store into each of them. The switch landed in [open-software-factory/software-factory#143 (move the JavaScript packages to one pnpm workspace)](https://github.com/open-software-factory/software-factory/pull/143).
5. **Built output.** No built bundle is committed. The design lab builds its pages with Vite, like the real app.
6. **Working mode.** The design workspace becomes a git worktree of this repo on a branch. OpenDesign opens that worktree as its project folder. Commits become pull requests, and the copy step and its promote script retire.
7. **Work items.** Still open. Tracker items and owner questions are to become issues under a `console` workstream with a `design` label. That changes the public board, so it waits for owner approval.

### Placed surfaces now win over per-view memory (fix, `2026-09-25`)

Two bug reports came from the owner. First, moving a centre view into the right pane, then picking another view, dropped the moved surface. The nav then listed it back under its primary section. Second, moving the run output surface into the left pane, then switching views, showed it back in the right pane. That is where the target view last remembered it.

The cause was `reduceWorkspace` in `packages/console-model/src/views.ts`. A view switch replaces the side panes wholesale with the target view's memory. A surface the operator just moved was either missing from that memory or remembered in its old slot.

The rule now is this. A surface the operator moves into a pane stays there across every view. It stays until the operator closes it or moves it again. Per-view memory still covers the panels opened during work.

The fix adds a `placed` field to `Workspace`. A move records or clears an entry. Any other action drops an entry once its surface leaves the recorded slot. After a view switch restores memory, every placement is reapplied on top of it. 7 new tests cover both bug reports, closing a placed surface, moving one back to the centre, and the persistence round trip.

## Review fixes (`2026-09-02` DESIGN.md review)

| # | Section | Fix | Status |
|---|---|---|---|
| F1 | §3.1 | Direction B is only the **starting** shell, open to change as the design continues. The shell must support contextual focus on the attention queue and other surfaces as needed. Exploration tables, directions A and C, archived to `design-explorations.md`. | applied 2026-09-02 |
| F2 | §3.2 | "Six conceptual surfaces" was confusingly unnamed, and is now named in place. "Surfaces are not nav destinations" was downgraded from [E] to open, since it is too early to decide. | applied 2026-09-02 |
| F3 | §4.2 | Zone sizes are starting defaults that may change. All panels are resizeable, closeable, and persisted per §16.2. | applied 2026-09-02 |
| F4 | §9.2 | Focus-ring spec was wrongly tagged [E]. Downgraded to [C], to refine later. Only "a clearly visible focus indicator exists" stays binding (§10). | applied 2026-09-02 |
| F5 | §11.2 | Work-item node grammar confirmed unfinalised. Subtask-as-small-square rejected, because it does not scale to about 20 children. Explore aggregate progress on the parent, either a bar in the style GitHub uses, showing `n/m`, or a segmented donut in the style used by Linear, a project-tracking tool, showing a count. Children render individually only on focus or zoom. | doc updated; exploration pending |
| F6 | §11.2, §11.3 | Glyphs and edge styles must be visible in the doc, shown as real characters rather than words alone. Unicode approximations added to both tables. A true rendered glyph sheet ships with the F5 exploration, in HTML, using a real monoline set and state fills. | applied 2026-09-02 |
| F7 | §8.2, §6.3, §12.1, §12.2 | Full-doc sweep for word-only glyph descriptions. Visible characters added everywhere. While sweeping, PAUSED and BLOCKED were found to both specify a pause shape (‖), a clash flagged in §12.1, to resolve in the F5 glyph sheet. Other docs checked clean. | applied 2026-09-02 |
| F8 | §12 | State vocabularies overlap the factory domain model, which is not finalised. A scope note was added, since §12's state lists are provisional prototyping stand-ins. The design system owns only the encoding rules (colour+glyph+label, shape-identifies-state, severity-to-channel). Re-derive the lists from the domain model when it lands (Q14). | applied 2026-09-02 |
| F9 | whole doc | DESIGN.md must not reference ephemeral tracker items. All inline F/Q numbers were stripped, and openness is now marked with plain [O] tags. | applied 2026-09-02 |
| F10 | whole doc | The final review batch included a reorganisation. The prototype filename was removed from the components section. Old §14, prototype conventions, was absorbed into the doc header as working conventions. Old §15 stub was removed, since the legend now points here. The "owner decisions" wording was dropped. Toast one-at-a-time was put on hold (Q15), since several at once may be allowed, dismissible individually and as a group. Sections were reordered so contracts and canvas grammar lead and element detail follows. | applied 2026-09-02 |

Section renumbering (F10):

| Old | New |
|---|---|
| 16 | 5 |
| 17 | 6 |
| 18 | 7 |
| 19 | 8 |
| 20 | 9 |
| 11 | 10 |
| 12 | 11 |
| 5 | 12 |
| 6 | 13 |
| 7 | 14 |
| 8 | 15 |
| 9 | 16 |
| 10 | 17 |
| 13 | 18 |
| 14+15 | header |

Rows F1 through F9 use the **old** numbering.

| # | Section | Fix | Status |
|---|---|---|---|
| F11 | DESIGN.md §5.2, components.md | Requirement widened. Panels now grow into a workspace, with tabbed panel groups, left, right, and bottom panes around a tabbed centre, and pop-out windows. A focus mode collapses everything and restores it exactly. SidePanel, the earlier side-panel component, was renamed **Workspace**. The survey verdict was revised from "build" to "adopt Dockview", since the old verdict answered the narrower one-panel requirement. Two things need verification before committing to this. Animated layout transitions need checking, since Dockview reflows instantly by default. Pop-out windows inside Tauri also need checking, where `window.open` becomes webview windows. | applied `2026-09-04`. The spike is complete, and the verdict is to adopt with two adaptations, in `survey/06-workspace-spike.md` and `workspace-spike.html`. Two things remain before final adoption. The owner tries the spike in a real browser, especially the animated-transition hack, which targets undocumented Dockview internals. Tauri `WebviewWindow` pop-out also needs verifying in the app repo. Document PiP, the browser's picture-in-picture feature, covers Windows and `WebView2` only, Microsoft's web-rendering engine. It does not cover `WKWebView` or `webkit2gtk`, the other engines. |
| F12 | components.md | Adoption rule codified. No requirement compromises. Spike every adoption against its contract rules before finalising. Adapt if possible. Fork-and-add if not. Never weaken a requirement to fit a library. The Dockview spike (`workspace-spike.html`) is the first application, covering theming, interaction lock-down, animated transitions, overlay-to-pin, focus mode, and the pop-out approach, where window.open was rejected by the owner. | applied `2026-09-04` |
| F13 | `workspace-spike.html` | **Layout animation, resolved.** It took six tries to find the real cause. `CSS-transition`, animating via CSS transitions, could not work, because Dockview positions `.dv-view` with inline `left/top/width/height` but rebuilds branch nodes on add/remove, so the element that should animate is often new and has no previous value to interpolate from. Dockview's own `.dv-animation` class is dead code. The string appears nowhere in `dockview-core@8.2.0`'s JS. It was replaced with a FLIP animator driven from `onDidLayoutChange`, which fires after the DOM mutation and before paint, plus a ghost layer that fades a clone of a departing pane. **The cause was ours.** This environment reports `prefers-reduced-motion: reduce`, rather than a bug in Dockview's own code, and a `MOTION_OK` guard silently turned every animation into a no-op. The OS preference now sets only the default state of the toggle, and no longer vetoes at runtime. | applied `2026-09-04`, verified by an in-page self-test, `SELFTEST` in the spike and default off. It covers close/reopen pane, float, dock, open pane, resize, and both tab switches, all reporting PASS with animation counts and durations. |
| F14 | `workspace-spike-panels.html` | **Second workspace spike.** This uses react-resizable-panels 4.12.3 and React 18.3.1, with no dock library. Built after comparing against the Claude Design shell, which reached better motion with plain CSS because it owns its DOM. Layout animation turned out to be **one CSS rule**, `[data-panel] { transition: flex-grow … }`. It measured interpolating from 6.9 to 3.2 to 0.8 to 0.1 to 0 across a collapse. Two traps were found and fixed. First, in v4 a bare numeric `defaultSize` means **pixels**, and pixel panels lay out via flex-basis with `flex-grow:0`, so nothing animates. Percentage strings (`'18%'`) are required for flex-grow sizing. Second, an inline `ref` callback that starts an animation re-runs on **every** render, which kept the centre canvas pinned near opacity 0. | applied `2026-09-04`, verified by frame-sampling computed style rather than by counting events. |
| F15 | `apps/console-lab/shell.html` | **Closing a pane must keep navigation.** The pane ✕ called `closePane`, which set the whole left region to `hidden`, taking the nav rail with it. The rule now: the ✕ closes pane content, and only the region toggle removes a region. Left collapses to `rail`, where the switcher survives. Right collapses to `hidden`, since it has no rail. Emptying a pane by closing its last tab follows the same rule, instead of leaving a zero-width husk whose mode still claims to be visible. | applied 2026-09-10, self-test suite B, six checks |
| F16 | harness | **The exporter snapshots after boot.** It captures about 660 ms in, measured with a painted clock in the self-test box, earlier runs simply captured mid-suite and looked like a silent harness failure. A suite must therefore finish inside that window. Motion is shortened for the run via `window.__MOTION_MS`, and the suite is split, `SUITE = 'A'` regression and `'B'` close/collapse, across two renders. Keep the first wait at ~200 ms, since a shorter one reads pane widths while the boot animation is still running and produces a false FAIL. | applied 2026-09-10 |
| F17 | DESIGN.md §5.5, `apps/console-lab/shell.html`, `data/nav.json` | **The nav conflated destinations with panels.** It listed Floor, Board, and Runway alongside Attention, Flight recorder, and Costs at one level, and set the same `aria-current` for the active centre view *and* the open left panel, so two items read as equally selected. Split into two kinds with two vocabularies. **view** uses radio, `aria-current="page"`, an accent marker, exactly one current, and a dim marker for open-but-not-current centre tabs. **panel** uses `role="switch"`, an open or closed indicator, any number open, and no accent marker. Each panel declares its own slot in the data, `attention` maps to left, `recorder` to bottom, and `logs` to right. Opening one never changes which view is current. `Costs` was dropped, since it had no surface behind it. `opensView` is retained in `nav.json` purely so the superseded spikes still boot. | applied 2026-09-10, self-test suite C, eight checks |
| F18 | DESIGN.md §5.5, `apps/console-lab/shell.html` | **Nav section now comes from placement.** Owner asked for (a) a visible rule separating primary nav from panels and (b) the nav to re-section an item when a surface is dragged between slots. (b) forces a model change: `kind` in `nav.json` stops being an identity and becomes a *home* hint. `api.placementOf(id)` scans centre, left, right, and bottom. `api.sectionOf(id)` returns primary when the surface is in the centre, and panel otherwise, falling back to home while closed so a shut surface does not drift between sections. Nav and rail both render from that derivation. Added `api.moveSurface(id, slot)` as the single relocation entry point, using a **Move to…** menu today, and drag-and-drop will call the same function later, so placement rules stay in one place. Nav items FLIP between sections rather than jumping. Two states this exposed and now handles: an empty centre, real once surfaces can be moved out, and the centre's ✕, which previously fell through to clearing the bottom pane. | applied 2026-09-10, self-test suite D, eight checks |
| F19 | `packages/console-model/`, `apps/console-lab/shell.html`, components.md | **Shell logic productionised into tested TypeScript.** The placement model left the prototype and became `packages/console-model/src/{types,placement,layout,geometry,persist,surfaces}.ts`, with no React and no DOM, and with 53 tests in `packages/console-model/test/*.test.ts`. Toolchain: none. Node 22 runs `.ts` tests natively (`node --test test/*.test.ts`), and the same source compiles into the page (first by a hand-rolled build script, now by the Vite build in `apps/console-lab`). The prototype now holds one `useReducer` over the model's `reduce` and no layout logic of its own, so the HTML cannot drift from the tested code. `SurfaceSpec.accepts` was added, permissive across every surface and every slot by decision, with the Move menu showing refused slots disabled rather than hidden. | applied 2026-09-10, 53 TS tests plus browser suites A, B, C, and D re-run after the refactor |
| F20 | `apps/console-lab/shell.html`, harness | **Animation shipped off, but tests passed.** Seeding motion from `prefers-reduced-motion` is correct in the product, but this preview host reports `reduce`, so the delivered artifact opened dead. The tests missed it because the harness *clicked motion on before every suite*, a workaround that is not testing the product. Three fixes were made. First, the design artifact starts with motion on regardless of host, with the reason stated in code, while the product keeps OS seeding (unit-tested). Second, motion off now shows a **visible one-click pill** in the top bar, since a disabled capability must never be silent, the same rule that §5.1.5 already stated and that this violated in a new shape. Third, the harness force-on is deleted, and suite A's first assertion is `animation on by default`. | applied 2026-09-10, suite A 11/11, suite E 9/9 |
| F21 | `apps/console-lab/shell.html` | **Scaffolding out, product controls in.** The motion toggle is gone, and nothing can switch it off. The A/B/C/D preset switcher is gone too. All four states are now reachable through the UI: a rail **expand** control opens full navigation, nav switches open panels, a pane's collapse control returns it to the rail, and **focus mode** is a real mode in the top bar with Esc to leave and exact restore. Tab treatment is hardcoded to **chip**, and underline and lifted were removed and archived. The viewport simulator stays, since it is the only way to exercise breakpoints in a fixed-size preview. The suites were rewritten to drive these controls instead of the preset buttons, the same rule as F20: if a suite cannot reach a state through the UI, neither can the operator. | applied 2026-09-10, suites A 11/11, B 7/7, C 8/8, D 8/8, E 11/11 |
| F22 | harness | **Flaky test fixed at the cause.** `centre absorbs space` failed intermittently and passed on re-run. The renderer resizes its own viewport during boot, measured from 1926 to 1443, so the panes could be at rest while the space around them still changed. The harness now waits for the stage *and* the panes to hold steady for three frames rather than waiting a fixed 200 ms. | applied 2026-09-10 |
| F23 | DESIGN.md §5.5.7, `apps/console-lab/shell.html` | **Navigation lost again.** This time it was fixed at the rule, rather than just the instance. F15 stopped the panel ✕ from taking the rail, but the region toggle `◧` still set the whole left region to `hidden`, and the rail lives in that region. Both were instances of the same missing rule, now written down: navigation is not a panel, and no ordinary control may remove it. The region toggles show and hide **panels**. `◧` now closes the left panel and restores it, remembering what was there. Navigation goes only through focus mode, which is explicit and reversible. **Suite F** exists solely to sweep every control against this: panel ✕, region toggle, width change, and focus in and out. It should have existed after F15. | applied 2026-09-10, suite F 6/6 |
| F24 | `apps/console-lab/shell.html` | **The navigation width animated wrong.** The outer column animated, so the test passed, but the rail's own wrapper had its width set instantly. The two faces were rendered as `if (wide) … else …`, so the contents reflowed and swapped in a single frame *inside* a container that was still moving. Fixed by animating the rail wrapper on the same clock, and by keeping **both faces mounted, stacked and clipped**, cross-fading between them. Icons at 72px and a labelled list at 248px are laid out differently, so there is no honest morph. The only smooth option is to trade places. The face that is fading out carries `inert`, so an invisible navigation cannot be tabbed to or clicked. **Suite G** measures all three properties across frames. Measuring only the outer column is what hid this. | applied 2026-09-10, suite G 5/5 |

**Harness rule (from F20).** A self-test may not put the app into a state the user would not get on open. If a suite needs a precondition, it must reach it through the same controls the operator has, after asserting the default. It must never be set up silently.

New review findings get the next F number. A fix is `applied` only after the DESIGN.md edit exists.

## Repository shape

The target shape, agreed on 25 September 2026, follows. The console restructure moves the current folders into it.

```
Cargo.toml             Rust workspace list: crates/*, apps/*/src-tauri
package.json           pnpm workspace root only, with no code
pnpm-workspace.yaml    apps/*, packages/*, integrations/*
crates/                the engine: osf and the crates it runs
apps/console/          the real app: React and Vite, with src-tauri/ for the desktop build
apps/console-lab/      the design lab under Vite: prototype pages and sample data. It never ships
packages/console-model/  the placement model, TypeScript with no React and no DOM
packages/console-ui/     the component package: tokens, overlay, palette, popovers, orb
integrations/          the agent-harness plugins
docs/design/           documents only
```

A server component, such as an API or MCP service, would go in `services/` when it exists. The browser build and the desktop build both come from `apps/console`.

## Verifying

```
pnpm --filter @open-software-factory/console-model test    # placement model, headless
pnpm --filter @open-software-factory/console-ui test        # components, jsdom + Testing Library
pnpm --filter @open-software-factory/console-lab build      # builds shell.html, orb.html, index.html
pnpm --filter @open-software-factory/console-lab dev         # live reload while editing
```

To look at a page, build the lab, then open the built file directly:

```
pnpm --filter @open-software-factory/console-lab build
```

Then open `apps/console-lab/dist/shell.html` or `apps/console-lab/dist/orb.html`. Each is one
self-contained file with no server and no external script or stylesheet reference, so it opens
straight from disk.

**Versioning.** The design work is versioned in this repo. Each landed slice is a commit on a branch and reaches `main` through a pull request. OpenDesign writes host folders into its project folder. The worktree's ignore rules must keep those folders out of commits.

Toolchain: **Vite 8**, with `@vitejs/plugin-react` for JSX and `vite-plugin-singlefile` to inline every script and stylesheet into one HTML file per page. Tests run on Node's own runner with native TypeScript. Model tests need no DOM. Component tests use jsdom and Testing Library.

Four guards run on every build, each earned by a real failure:

| Guard | Why it exists |
|---|---|
| No Web Storage in the built output | OpenDesign's preview scans the artifact and forces a sandboxed render when it finds one, breaking relative imports. Caught its first violation immediately, a doc comment, since comments survive stripping. A Vite plugin checks this now, in `apps/console-lab/vite-plugins/output-guards.js`. The spikes are exempt: they predate this rule and use Web Storage by design |
| No hard-coded colours in hand-written CSS | Caught the canvas depth gradient, which kept its dark values and turned the centre black under the light theme. Diagnostics opt out explicitly with `od:theme-exempt` markers. A node test in `packages/console-ui/test/no-literal-colours.test.ts` scans the packages' CSS and the lab's hand-written CSS |
| The build parses | Vite fails the build on a syntax error, so a broken page never reaches `dist/` |
| No CommonJS `require()` left in the built output | The same Vite plugin that checks Web Storage also checks this. A leftover `require()` from an unbundled dependency throws at load in the browser, which is a blank page with no explanation |

Browser-level suites live in `apps/console-lab/src/shell/selftest.js` behind `SELFTEST`, default off. They cover:
- A, layout regression
- B, close/collapse
- C, nav grammar
- D, placement to nav section
- E, theme/focus
- F, navigation survival
- G, nav animation
- H, bottom region/narrow drawer
- I, subject labels
- J, shared Overlay
- K, command palette
- L and M, inspector float/pin/unpin

They must finish inside the exporter's snapshot window of about 640 ms. This is why motion is shortened during a run and the suites are split. J now awaits actual motion completion. A capture taken before `done` is incomplete rather than a pass.

**Measurement rule, from F14.** Counting `transitionstart` events produced false
FAILs while the layout was demonstrably animating. Sample the computed value
across frames instead. If it interpolates, it animates. Event tallies show that
a call happened. They do not show that motion did.

**Verification rule, from the F13 incident.** When a change cannot be shown in a
static render, build a self-test into the artifact. The self-test must drive
the real interactions and print PASS or FAIL into the page. Render the page
once, then read the result. Do not deliver motion or interaction work with
"needs your eyes".

## Open questions registry

Q numbers stay stable. This registry was formerly DESIGN.md §15.

| # | Question | Why it matters |
|---|---|---|
| 1 | Operator load profile, locked hypothesis: single factory, 30 to 100 items, 5 to 20 events per day | Density and shell feasibility assumptions |
| 2 | Intervention vocabulary, tested set: `approve`, `request-changes`, `clarify`, `redirect`, `stop`. `pause` is carried-only, and `queue` was dropped. This splits into **judgement**, covering approve, request-changes, and clarify, and **operation**, covering redirect and stop, which need confirmation and an audit. Confirmation model and undo drafted in DESIGN.md §8 [C]. Still open: whether redirect needs a target/approach picker. | ADR mandates read and control together. Verbs are now derived from decision points instead of being named in the abstract |
| 3 | Multi-operator state: shared attention, claiming, assignment? | Shell and attention-item lifecycle depend on it |
| 4 | Out-of-band presence: notifications, sounds, idle detection? | Behaviour framed as being on call rather than on shift |
| 5 | Product-loop authoring in console: show specs as evidence only, or edit in place? | Review surface and focus path scope |
| 6 | Replay semantics: how far back, what aggregation, what granularity per surface | Time-as-axis implementation |
| 7 | "Unusual changes from baseline" definition (window, threshold, grouping) | The quiet healthy summary needs anomaly semantics |
| 8 | Final typography: Plex vs Geist vs an alternative | Test in prototype against alternatives |
| 9 | Exact palette and contextual tinting (mode-level hue shifts) | Session direction: blue-green base, dual accent (blue + teal) + mono variant, **no purple**, pastel-toned, tasteful dark + a light theme. Exact values and hue shifts left as exploration |
| 10 | Work board: does it earn a place beyond graphs + runway? | Session hypothesis. Test as a lens, likely not a surface |
| 11 | Light theme requirement, implemented in the shell | Both dark and light are generated from the tested token layer, dark default, light toggle. Final aesthetic refinement remains open |
| 12 | Ambient-assistant UI surface forms: annotation, generated investigation view | Cross-cutting modality. Interaction model open |
| 13 | Do the six conceptual surfaces map to modes, nav destinations, or a mix? Reopened by F2, the current mode/lens/dock shape is a working hypothesis rather than a settled one | Shell information architecture |
| 14 | Factory domain/data model, covering state taxonomies, transitions, and entity relationships, is not yet finalised. It is owned by the `software-factory` product work rather than the design system. When it lands, re-derive DESIGN.md §11's vocabularies, the §6.2 terminology table, and the fixture data from it | §11 encodings, node grammar, fixtures, and StreamSurface schemas all bind to it |
| 15 | Toast stacking: one at a time vs several; decide after the component survey. Fixed requirement either way: each toast dismissible individually **and** the group dismissible in one action | DESIGN.md §7.1 channel taxonomy. Toast primitive contract |
| 16 | ~~Frontend framework~~ **Decided.** React and TypeScript were chosen, with desktop via Tauri, on `2026-09-10`. Consequences: build-order step 1, the Overlay primitive, adopts **React Aria Components**, with `Ark UI/Zag` only the hedge for staying framework-open. Cytoscape, AG-UI, and the voice engines were framework-neutral and are unaffected. Tauri implications already logged: pop-out uses `WebviewWindow` instead of `window.open`. Document PiP is Chromium/WebView2-only, so it is a Windows-only enhancement at best. The local STT default differs per platform webview (`survey/05`) | Closed. This unblocks build-order step 1, the Overlay primitive |
| 18 | **Toolchain decided**, verified against current releases rather than memory, on `2026-09-10`. TypeScript **7.0**: the Go-native compiler is now plain `tsc`, so there is no `@typescript/native-preview`. `tsgo` today means only the nightly channel. Bundler **Vite 8**, which ships **Rolldown**, Rust and Oxc-based, as its single bundler for both dev and build. Vite no longer uses esbuild. Lint **oxlint 1.x**. Format **oxfmt**, beta as of February 2026, so its status needs confirming at setup, with a fallback to Biome or Prettier. Consequence for the design loop: build the single-file design artifact with **Rolldown** rather than esbuild, so the design and production builds share one transform engine | Toolchain for `packages/*` and the design build |
| 19 | **Loops**, owner, `2026-09-11`. SDLC, product, and meta loops as operator-defined workflows with steps and optional/mandatory human gates. Loop views as task flow and/or state machine with live work-item positions overlaid. Zoom into a task/state for sub-flows. Needs a loop-definition schema before any surface. See "Owner direction, loops as an organising idea" under Next steps | Reframes Q17 and the canvas engine choice (B3). The centre becomes a set of loop views with a zoom ladder |
| 17 | Infinite-canvas centre: "everything is somewhere on one infinite canvas". Owner to describe. Blocks the canvas-tech adoption (Cytoscape + ELK), because a general spatial canvas and a graph-layout engine are different products | Canvas adoption, MotionViewport, §5.7 label floor, zoom ladder |

## Shell contract coverage

This audit ran on 10 September 2026. Rule-by-rule state of `DESIGN.md` §5 against what is actually built and verified. "Proven" means a test asserts it. The test is either a TS test in `packages/console-model/test/` or a browser suite in `apps/console-lab/shell.html`.

| Rule | State | Evidence / gap |
|---|---|---|
| 5.1.1 new info must not shove the layout | **built and proven** | Two compliant forms, chosen by the **trigger**: operator-requested (nav, palette) docks directly as a panel that animates in; click-on-content floats the inspector as a non-modal Overlay, then pin docks it. `inspect.test.ts` proves the layout is untouched by a selection; suite L proves it in the browser (`layout not shoved`) |
| 5.1.2 screen/view change transitions | proven | Centre tab crossfade + directional slide; suite A `centre tab animates` |
| 5.1.3 menus, modals, search animate in | **partial.** Modal foundation and palette built | Narrow navigation drawer and the command palette both use Overlay; rendered entry/exit interpolation observed. Anchored menus still pending. Move menu remains retired; its keyboard twin lives in the palette |
| 5.1.4 zoom / pan / programmatic scroll animate | **not built** | `apps/console-lab/shell.html` has no zoomable surface. Floor is a static SVG. Partly existed in `factory-floor.html` |
| 5.1.5 motion | **built; policy reconciled 2026-09-11** | Motion is always on. `settings.ts` stores theme only. Suite E asserts motion is on and no motion control exists; the build rejects host motion-preference gates. Earlier OS-seeding and toggle descriptions are superseded |
| Light theme (Q11) | **built and proven** | `packages/console-ui/src/tokens.ts` holds both palettes from one base hue, derived not inverted. 42 tests including WCAG contrast computed from OKLCh: body text ≥ 7:1 on every surface in both themes, accent and state colours ≥ 3:1. Toggle in the top bar; the setting persists through the storage port |
| 5.2.1 pin / unpin | **built and proven** | Float the inspector, then press Pin, to dock it via the placement reducer. Dock the pane, then press Float, to lift it out with its subject. `inspect.test.ts` round-trips pin, unpin, then pin again. Suite L drives the controls |
| 5.2.2 closeable + resizeable | proven | TS `layout.test.ts` close/collapse rules; suite B; drag-resize in the prototype |
| 5.2.3 per-view persistence | **built and proven; rule amended 2026-09-12** | `views.ts`: each centre view remembers which side and bottom panels are open and which is active; widths and navigation width are global. 12 tests; suite Y |
| 5.2.4 workspace: tabs · rearrange · pop-out | **partial.** Tabs and rearrange built | Tabs proven; drag-rearrange of tabs and headers to region zones (suite Z). Pop-out: port only (A5); Tauri `WebviewWindow` unverified |
| 5.2.5 focus mode | **built and proven** | A real mode, not a preset: the top-bar control collapses every pane, keeps a visible pressed exit, leaves on Esc, and restores the exact prior layout. Suite E recorded `L 367→367 · R 344→344`. The snapshot is deliberately not persisted; a reload should not resume with everything hidden |
| 5.3 ambient assistant | not in this shell | v1 orb built in `factory-floor.html`; context-aware menu never built |
| 5.4.1 mouse / keyboard / touch parity | **audited 2026-09-11; mostly built** | Tablists with roving focus, keyboard-resizable grips, Alt+n views, and Escape and focus return on every overlay. DOM tests and suite W confirm this. Residue: tab close × not separately focusable; rail icons named by `title` |
| 5.4.2 voice on all inputs | **not built** | Engines chosen in `survey/05`; `VoiceInputAdapter` not written |
| 5.4.3 touch target minimums | **built for coarse pointers** | `(pointer: coarse)` raises icon controls, tabs, nav rows and grip hit zones to 44 px; mouse layout unchanged. Not rendered on a touch device yet |
| 5.5.1 omni-search on lists >10 | **partial** | The palette's ranked filter (`filterCommands`) searches board rows and runs globally; no per-list filter is attached to Board or Runway yet, and both exceed 10 rows |
| 5.5.2 breadcrumbs / jump back | **built for the inspector** | Source path with one-click jumps (suite AA). Deeper drills come with A9. They cover the focus path and evidence tiers |
| 5.5.3 Ctrl+P light-dismissable | **built and proven** | `CommandPalette` on the shared Overlay: Ctrl+P / ⌘K and the top-bar control open it. Escape, outside click, or running a command close it. Escape works in one step, even mid-query. DOM tests in `palette.test.ts`; suite K in the browser |
| 5.5.4 to 5.5.6 nav grammar + separator | proven | Suite C (8 checks); `placement.test.ts` |
| 5.5.7 surface declares, slot decides | proven | `accepts` enforced in the reducer and on restore; `placement.test.ts` |
| 5.5.8 kind follows placement | proven | Suite D (8 checks); `placement.test.ts` |
| 5.6 generated / streaming UI | **not built** | Standards chosen: AG-UI plus an A2UI-shaped schema. Neither a schema nor a surface exists yet |
| 5.7 text readability floor | **not built** | No zoomable surface here. Cytoscape's `min-zoomed-font-size` is the adopted answer. It is unspiked |

Roughly a third of the contract is proven, a third partly built, a third untouched.

**Convergence is decided.** `apps/console-lab/shell.html` owns the placement model, nav grammar, panes, motion and both themes. Extract the older floor's capabilities as components and re-host them as shell surfaces, following the convergence plan below. The four floor state files remain stale and are regenerated last.

## Component build status

This status is from 3 September 2026, against `factory-floor.html`, which is now superseded and kept for reference.

| Component | Status in prototype |
|---|---|
| Overlay | partial. Inspector and palette each carry their own ad-hoc version; palette light-dismiss must move into the shared Overlay |
| Workspace (was SidePanel) | partial. Inspector slides in and closes; no pin, resize, persistence, tabs, pop-out, or focus mode. Adoption: Dockview (F11), pending Tauri pop-out and animation verification |
| Dialog | partial. P0 panel exists; appears without motion |
| Popover | Not built as a shared component. Environment menu and assistant menu are ad-hoc |
| Toast | built |
| ScreenHost | Not built. Instant swap today |
| MotionViewport | partial. Zoom-reset animates; +/− and node-focus jumps snap; labels scale geometrically, violating rule 5.7 |
| FloatingOrb | built as the assistant; the menu is fixed today and needs context-awareness added |
| Breadcrumb | Not built |
| ListFilter | Not built as a shared part. The palette has its own filter |
| Button / IconButton | partial. Styles exist; focus rings and touch sizes not audited |
| TextInput | partial. No voice yet |
| SearchField | built, minus voice |
| Select | partial. Ad-hoc |
| TopBar · ModeRail · CommandPalette · ThemeToggle · AttentionIndicator · CriticalBanner · ZoomControl | built |
| EnvironmentScope | built. Menu should become a Popover |
| AmbientAssistant | built as v1 |
| StreamSurface | Not built. Lab has a static stand-in |
| PresetSaver | Not built |

## Next steps

So far these milestones are complete. The owner reviewed DESIGN.md, items F1 through F10, on 2 September 2026. The Direction-B floor prototype, the shell's first starting layout, is done. The component survey, `survey/00` through `05`, ran on 3 September 2026. The shell audit and owner calls happened on 10 September 2026. They covered Q16, rule 5.1.1, motion, and convergence. The Overlay build step, covering palette and inspector, finished on 11 September 2026.

What remains falls into two parts. Each item has an id, a size, and what it waits on. A size is S for a session, M for a few sessions, or L for a phase. Owner sets the order against the ids.

These items are done and proven, for reference:
- placement model
- nav grammar
- panes, tabs, resize
- focus mode
- both themes
- motion policy
- shared Overlay, modal and non-modal
- command palette
- inspector: select, then float, then pin or unpin

### Part A: finish the shell

| Id | Step | Contract | Size | Waits on | Note |
|---|---|---|---|---|---|
| A1 | ~~**Popover** primitive: anchored, flips near edges~~ **done 2026-09-11** for environment scope and the attention indicator; gate/deployment detail popovers wait on B4 | 5.1.3 | S to M | None | `popover.ts`; suite S |
| A2 | ~~**P0 decision Dialog** on the shared Overlay, with motion~~ **done 2026-09-11** | 5.1.3 | S | None | `interrupt.ts`; suites Q, R |
| A3 | ~~**Drag-rearrange** panes and tabs~~ **done 2026-09-11.** Tabs and headers drag to region zones; drop calls `move` | 5.2.4 | M | None | suite Z |
| A4 | ~~**Per-view layout persistence**~~ **done 2026-09-11.** Side and bottom panes per centre view; navigation width stays global | 5.2.3 | S to M | None | `views.ts`; suite Y |
| A5 | **Pop-out.** Port and disabled control **done 2026-09-11**; Tauri `WebviewWindow` adapter and its verification still open | 5.2.4 | M | A Tauri host | `popout.ts`; never `window.open` |
| A6 | ~~**Keyboard, touch and target audit**~~ **done 2026-09-11.** Tablists, keyboard grips, Alt+n views, coarse-pointer targets, narrow top bar; residue listed in the checkpoint | 5.4.1, 5.4.3 | M | None | suite W |
| A7 | **ListFilter** on Board, Runway and Attention when over 10 rows | 5.5.1 | S | None | From the findability build step, ListFilter and Breadcrumb. Reuse the palette's ranked filter |
| A8 | ~~**Breadcrumb / jump back**~~ **done 2026-09-11** for the inspector's source path; deeper drills come with A9 / B4 | 5.5.2 | S | None | suite AA |
| A9 | **Inspector focus path**: intent, then spec, then item header, evidence tiers, actions | components L4 FocusInspector | M | B5 for real evidence shape | The inspector body today shows only what the source list knows |
| A10 | **MotionViewport**: zoom, pan, programmatic moves animate; text readability floor. Brainstorm written 2026-09-11, see "A10 brainstorm, the centre as loop views". Proposed first step: loop-definition schema + Floor redrawn from it | 5.1.4, 5.7 | L | Owner reaction to the brainstorm; Q17 | Floor is a static SVG today |
| A11 | **Voice input** adapter on every text input | 5.4.2 | M | Engine choice from `survey/05`; local STT per platform | From the TextInput voice build step |
| A12 | **Ambient assistant** orb and context-aware menu | 5.3 | M | Q12 surface forms | In the shell 2026-09-13: ring from the model, context intents, orb as the collapsed nav, assistant panel (no transport). Open: default look, transport (Q12) |
| A13 | **StreamSurface** for generated / streaming UI | 5.6 | L | Part schema first (AG-UI + A2UI-shaped) | From the StreamSurface plus PresetSaver build step |
| A14 | ~~**Floating inspector polish**: draggable, remembers position, narrow-viewport behaviour~~ **done 2026-09-11** | 5.1.1 finish | S | None | `floatpos.ts`; suites T, U, V |
| A15 | **Toast** in the shell, with the stacking decision | Q15 | S | Q15 | Built in the floor, absent from the shell |
| A16 | **Retire the floor**: regenerate the four state files from the shell, then archive `factory-floor.html` | convergence plan | S | A10, A12, B2 extracted | Last, by design |

### Part B: after the shell

| Id | Step | Source | Size | Waits on | Note |
|---|---|---|---|---|---|
| B1 | **Work-item node grammar**: subtask aggregate (bar vs donut), rendered glyph sheet, BLOCKED/PAUSED shape clash | tracker F5, F7 | M | None | Never explored; blocked by nothing |
| B2 | **Flight recorder** design, then `RecorderTimeline` extraction | tracker Q6, convergence plan | M to L | Q6 replay semantics | Owner considers it underdeveloped |
| B3 | **Canvas engine** decision and spike, comparing Cytoscape + ELK against an infinite canvas | tracker Q17 | M | Q17 described | Do not spike before Q17 |
| B4 | **Layer 4 product components**: ReviewPanel, AttentionStrip ranking, RunwaySummary verdict, GateBadge/DeploymentPill, EmptyCalmState, StaleIndicator | components.md | L | A1 for popovers, B5 for data | One state grammar for node and card forms |
| B5 | **Domain / data model**: state taxonomies, transitions, entities; then re-derive DESIGN.md §11, §6.2 and the fixtures | tracker Q14 | L | Product work outside the design system | Everything in B4 and A13 binds to it |
| B6 | **Owner decisions still open**: Q1 load profile · Q2 intervention vocabulary · Q3 multi-operator · Q4 presence · Q5 authoring · Q7 anomaly definition · Q10 board's place · Q13 surfaces vs modes · Q8 typography · Q9 tinting | tracker registry | None | You | Q13 reshapes the nav; Q2 shapes the inspector actions |
| B7 | **Doc hygiene**: strip validated [C] tags in DESIGN.md; fold the superseded floor status table; keep the tracker checkpoint current | This tracker's doc-hygiene backlog | S | None | |
| B8 | **Production path**: promote `packages/*` to the product repo, TS 7 / Vite 8 / oxlint setup, Tauri shell | tracker Q16, Q18 | M | Repo exists | the console restructure landed the packages and the Vite app in this repo; the desktop shell is still open |

### Owner priority

This priority is from 11 September 2026.

1. **Shell, in this order:** A1, A2, A3, A4, A5, A6, A8, A14. Do a polish pass first. Bring the horizontal gap between panels down to match the narrower vertical gap.
2. **Then** brainstorm A10 and the canvas. Q17 likely goes with B3. See "Owner direction, loops as an organising idea" below.
3. **Not shell work:** A7 (per-list filter) does not belong in the shell for now. A11 (voice) and A12 (ambient assistant) move to the components track as global shared components. A12 may still count as shell.
4. **Needs thought and exploration first:** A13 (streaming UI), A15 (toasts).
5. **A16** (retire the floor) once the above is done. The floor's "surfaces" will likely become one or more views per surface, and more. See below.

### Owner direction, loops as an organising idea

This direction is from 11 September 2026.

The factory supports several development workflows, seen as **loops**. A default is the **SDLC loop**. An outer one is the **product loop**. A **meta loop** covers the factory's own self-improvement. Operators define and tweak the SDLC loop first, then product and meta loops later. They add steps, leave steps out, place human approval gates, and mark gates optional or mandatory.

Surfaces then visualise each loop separately and together, with real-time data overlaid to show which stage one or more work items are at. A loop can be drawn as a task flow, as a state machine, or both. Each task or state can be zoomed into to reveal sub-states or sub-flows with the relevant work, agent and status data.

These are the consequences to carry into A10, Q17, and B3. The centre is not one canvas. It is a set of loop views with a zoom ladder. The engine must support hierarchical, zoom-in, graphs. It must overlay live item positions on a defined flow. Loop definitions are data the operator edits, so they need a schema before a surface. This is recorded as **Q19** in the registry.

### A10 brainstorm, the centre as loop views

This brainstorm is from 11 September 2026, for Q17, Q19, and B3.

This starts from the owner's loops direction rather than from a canvas engine. These are ideas awaiting a decision. Each names what it would need.

**1. What the centre shows.** Not one infinite canvas but a small set of *loop views*, each a defined workflow with live work overlaid. Three loops begin the set: SDLC, the default. Product is the outer loop. Meta is the factory improving itself. A loop view answers "where is work in this loop right now?" The floor's current node graph is a rough draft of the SDLC loop, drawn as a task flow.

**2. Two drawings of one loop.** A loop has a *task-flow* drawing. Its steps run in order, with gates between them and branches for optional steps. A loop also has a *state-machine* drawing. This shows the states a work item passes through, with transitions as edges. Both are the same definition in two projections. The operator toggles between them, the way Floor's lenses were meant to work. Live items are the same dots in both. This needs one loop-definition schema that both projections read. That schema holds steps, gates marked optional or mandatory, allowed transitions, and who may approve.

**3. Zoom ladder as drill.** Drilling changes the level of detail. The scale of the drawing stays the same. Zooming into a step reveals its sub-flow or sub-states, with the work and agent status inside. Zooming out collapses it to a single node carrying an aggregate: a count of items, the worst state, and the oldest wait. This is the §5.7 readability floor in practice. Labels never shrink. Only the level of detail changes. This needs hierarchical loop definitions, where a step may contain a loop, plus per-level aggregates. The Breadcrumb (A8) becomes the drill trail.

**4. Live overlay.** Items are positioned by their current step or state, drawn automatically from that data. Movement between steps animates along the edge, a §5.1 rule applied for free. A stalled item earns the attention halo from the floor grammar. Gates show their queue. This needs an event feed that says "item X entered step Y at T." That is the same feed the flight recorder would replay, under Q6. This is why B2 and this idea share a substrate.

**5. Operator-defined loops.** Add or remove a step, place a human gate, mark it optional or mandatory, and the view redraws. Editing happens in a *definition* surface, a form or a structured editor, rather than by dragging nodes on the live view. The live view is read-and-intervene. The definition is authored. Meta-loop changes to the factory's own loop would need approval gates of their own. This needs schema versioning and a way to say which definition version an item is running under.

**6. Together and separately.** Each loop has its own view. A combined view nests them. The product loop sits outside, the SDLC loop sits inside a product step, and the meta loop sits off to the side, acting on both. The zoom ladder crosses these loop boundaries. This is the one place an infinite canvas, Q17, may still be the right container. It would be a pannable field with three loop drawings at fixed positions, rather than a free-form space.

**7. Engine implications (B3).** Requirements the engine must meet before adoption:
- hierarchical, compound, nodes with expand and collapse
- layout generated automatically from a definition
- animated position changes
- two layout algorithms, layered for task flows and force or layered for state machines
- label counter-scaling

Cytoscape and ELK cover compound nodes and layered layout. ELK does not animate, so transitions are ours. A hand-rolled SVG, what Floor is today, stays viable for a loop with under about 40 nodes and no free panning. Spike both against the zoom-ladder-as-drill requirement first, because that is where hand-rolled SVG will break.

**8. Smallest useful next step.** Build a loop-definition schema with the default SDLC loop as a fixture. Redraw the Floor from it as a task flow, with today's items overlaid by state. This needs no engine change yet. The zoom ladder can follow once the drawing is data-driven.

### Build order

This order moved here from `components.md` on 11 September 2026.

Each step unlocks the ones after it. Do not start a later step by re-implementing an earlier one inline. Where the survey finds an adoptable primitive, a step becomes "adopt and theme" instead of "build".

0. **Placement model.** *Done.* `packages/console-model/src/*.ts` plus its tests. Everything below dispatches actions against it rather than holding layout state of its own.
1. **Overlay.** *Done.* Modal foundation, the navigation drawer and command palette, plus a non-modal contract, the floating inspector, both on one motion lifecycle. The Move menu stays retired. The palette's Move commands are its keyboard twin. Inspection state, in `packages/console-model/src/inspect.ts`, derives "docked" from placement and dispatches pin/unpin to the layout reducer.
2. **Workspace.** This is the pane/tab/resize shell over the placement model, plus drag-and-drop (A3) and per-view persistence (A4). Drop targets call `move`. They add no placement rules of their own. Dockview stays a candidate for the tiling engine only. Pop-out (A5) runs inside Tauri, using its `WebviewWindow` API instead of `window.open`.
3. **ScreenHost and MotionViewport**, the two whole-screen motion primitives, remove the last instant swaps. As A10, MotionViewport waits on Q17 and Q19.
4. **Popover and Select.** Dropdown menus and detail popovers converge here (A1).
5. **ListFilter and Breadcrumb**, the findability pass. A8 needs this in the shell. A7 stays parked as work outside the shell.
6. **TextInput voice**, one mic behaviour, applied to SearchField, palette, assistant (A11, components track).
7. **StreamSurface and PresetSaver**, a generated-UI surface and a preset-saving control, need a fixture of streamed parts. Design the part schema first (A13, exploration first).

## Prototype convergence plan

This plan was agreed on 10 September 2026.

`apps/console-lab/shell.html` is the base. It owns the tested placement model. `factory-floor.html` is not merged into it. Its capabilities are extracted as components and re-hosted as surfaces inside the shell. Order and rationale:

| From the floor | Becomes | When | Note |
|---|---|---|---|
| Light theme | Token layer in the shell, then applied back to the floor | **Shell complete.** Generated palettes and theme toggle | Applying the shared token layer back to the older floor is not yet verified |
| Ambient orb | `FloatingOrb` plus `AmbientAssistant`, layer 1 and layer 3 | With component work | Independent of the workspace; can proceed in parallel |
| P0 interruption | `Dialog` on `Overlay` | After Overlay | Currently appears with no motion |
| Flight recorder | `RecorderTimeline` (layer 4) | **After design work.** The owner considers it underdeveloped. Design it properly, then extract it, then reuse it | Also bound to Q6 replay semantics |
| Canvas / graph | `MotionViewport` + canvas engine | **Blocked on Q17.** The infinite-canvas idea may change the engine choice entirely | Do not spike Cytoscape before Q17 is described |
| Four state files | Regenerate from the shell once surfaces land | Last | Stale today; do not maintain them in the meantime |

Retiring the floor would lose several things unless they are carried over deliberately:
- the zoom ladder and its density rules
- the attention halo and pulse grammar
- the P0 interruption model
- evidence-tier navigation in the inspector
- the live/replay pairing between canvas and recorder
