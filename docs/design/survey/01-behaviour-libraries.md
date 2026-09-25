# Survey 01: headless and behaviour-first libraries

Scope: Overlay, Dialog, Popover, Toast, ListFilter, a filter control, Button, TextInput, a text field, SearchField, a search field, and Select. These are the Layer 1 and Layer 2 primitives named in `components.md`. The framework choice is not locked. Framework dependency is a scoring column here. It is not a filter. Evidence comes from web search, done in September 2026.

## Scoring table

A check mark means pass. A tilde means partial or depends. A cross means fail or gap. The weight column is not applied here. This is a side-by-side comparison. It is not a ranked score.

| Candidate | 1. Themeable, no baked visuals | 2. Light-dismiss and focus management | 3. No hard-coded breakpoints | 4. Accessibility built in | 5. Framework dependency and weight | 6. Mix-and-match | 7. Licence and 2026 health |
|---|---|---|---|---|---|---|---|
| **React Aria Components**, an unstyled accessible UI library from Adobe | unstyled. Uses render props, data attributes, and CSS variables only | own focus-scope, overlay, and positioning logic. Best in class | no layout CSS shipped | built by Adobe's accessibility team. Deep ARIA, keyboard, and touch support | React only. Modular and tree-shakeable | React only, but coexists with any CSS or styling layer | Apache 2.0 licence. Version 1.21 shipped days before this survey. Weekly releases |
| **Radix Primitives** | unstyled, using `data-state` attributes | solid, Popper-based positioning | no layout CSS | good ARIA and keyboard support | React only | React only. Huge existing ecosystem, including shadcn | MIT licence. Owned by WorkOS, a company. Contribution pace slowed during 2024 and 2025. Reinvestment is underway, but Base UI is now the recommended target for new work |
| **Base UI**, built by the team behind MUI, short for Material UI | unstyled, same model as Radix | built by former Radix engineers plus the MUI team | no layout CSS | strong. Newer components, including Combobox and NumberField, a numeric input, go further than Radix ever did | React only | React only. It became shadcn's new default in July 2026 | MIT licence. Version 1.0 went stable in December 2025. The `@base-ui/react` package is at 1.6, with over six million weekly downloads and an active release cadence |
| **Zag.js**, a framework-agnostic state-machine library for UI behaviour, used through **Ark UI**, its component layer | unstyled state machines. Framework adapters only emit state and attributes | dialog, popover, and toast machines handle focus trap and outside-click | no layout CSS. Positioning is delegated to Floating UI | ARIA baked into each machine | framework-agnostic. Works with React, Vue, Solid, and Svelte. Svelte support landed in 2026. It also runs in vanilla JS through the Zag core | best in class. Same behaviour contract regardless of the eventual framework choice | MIT licence, from the Chakra UI organisation. Actively maintained. The Svelte adapter is new in 2026 |
| **Floating UI**, a positioning engine | not applicable. This library is a positioning engine only | gives you `flip`, `shift`, and `autoPlacement`. No focus trap or dismiss logic itself | pure JS positioning, aware of the container and anchor | not applicable. It carries no ARIA semantics. It is plumbing | framework-agnostic core, with React, Vue, and DOM bindings. About 3 kB for the core | used internally by Radix, Base UI, and Headless UI. Safe to add under anything | MIT licence, 6.25 million weekly downloads, actively maintained. Native CSS anchor positioning, the `position-try-fallbacks` feature, is now Baseline and starting to absorb this use case |
| **Headless UI** | unstyled | decent, narrower component set. No Toast | no layout CSS | good for its covered set | React and Vue | fine alongside Tailwind-based work | no commit since April 2026, about five months stalled. Stable, but not actively developed. This is a risk for a multi-year console |
| **shadcn/ui**, a component-distribution tool | themeable by construction. You own the copied code, so tokens are trivial. It is a distribution model rather than a primitive. The underlying behaviour is Radix or Base UI | inherits whichever primitive it copied from | inherits it | inherits the underlying primitive's accessibility. Verify per copied component, since the code is now yours to maintain | React only. It copies source code into your repo | good. The copy-in model makes partial adoption or replacement easy | MIT licence, healthy. It drove the Radix-to-Base-UI migration itself, the July 2026 default switch |
| **Melt UI** and **Bits UI**, Svelte libraries | unstyled. Melt UI is the builder and actions layer. Bits UI is the componentised layer built on top | good. Melt UI handles focus and dismiss primitives directly | no layout CSS | solid | Svelte only | Svelte only. Relevant only if the framework choice becomes Svelte | MIT licence. Both are actively maintained in 2026 |
| **Web Awesome**, formerly Shoelace, a web-components library | ships opinionated default visuals and themes. Overriding these to OKLch tokens, a perceptually uniform colour format, means fighting shadow-DOM parts. This is not composing from nothing | has dialog, popover, and toast components with dismiss built in. Behaviour and visuals are fused. It is not a pure behaviour layer | no viewport breakpoints baked in | decent ARIA | true framework-agnostic, using native custom elements | works anywhere, but getting it themeable to zero is harder than with a headless React or Zag primitive | the core is under the MIT licence and is free and open source. The Shoelace repo was archived in May 2026 in favour of Web Awesome. It runs an open-core model, where a paid Pro tier funds it. This is healthy, but it is a business-model dependency to watch |

## Narrative

**React Aria Components** is the strongest single-framework contender. It has no shipped CSS at all. Styling is entirely our own tokens, applied through render props and CSS custom properties. Its overlay, focus, and dismiss logic covers `Dialog`, `Popover`, `Modal`, and the new `Toast` and `ToastQueue`. This logic is the most rigorously tested in the field, built by Adobe's accessibility team over decades of assistive-technology regression suites. The cost is the framework-agnostic option. We give that up if we ever want to leave React.

**Radix** and **Base UI** are functionally the same shape: unstyled, driven by `data-state`, and React only. They tell different maintenance stories in 2026. Radix slowed after its original team left WorkOS, a company, for other work. Base UI was built by many of those same engineers, plus MUI's paid team. It became shadcn/ui's own default in July 2026. It ships primitives Radix never had, including Combobox, a combined text and dropdown field, NumberField, a numeric input, and Checkbox Group. If we go React with shadcn-style ownership, Base UI is the safer 2026 pick over Radix.

**Zag.js** and **Ark UI** together are the one candidate that directly answers "framework not locked yet." Behaviour, meaning focus trap, light-dismiss, keyboard maps, and ARIA, lives once in a framework-agnostic state machine. React, Vue, Solid, and, as of a 2026 release, Svelte adapters are thin wrappers over the same logic. That means our Overlay, Dialog, Popover, and Toast contract could be validated once and survive a framework pivot. No single-framework library can offer that. The tradeoff is a less mature ecosystem than the ecosystems around React Aria Components, Adobe's library, or Radix. The API shape is also less familiar.

**Floating UI** is not a competitor to the other candidates in this table. It is the positioning primitive most of them already use for edge-flipping, through `flip`, `shift`, and `autoPlacement`. It is worth calling out on its own, because native CSS anchor positioning, the `position-try-fallbacks` feature, is now Baseline. It is starting to absorb exactly the edge-flipping job. This is worth revisiting as browser support solidifies inside the Tauri webview.

**Headless UI** and **Web Awesome** are both worth naming and ruling out, for different reasons. Headless UI has gone quiet, with no commits since April 2026, and it lacks a Toast primitive entirely. That is too much risk for a component we intend to run for years. Web Awesome, Shoelace's successor, is genuinely framework-agnostic, but it ships baked-in visuals through shadow-DOM parts. Getting it to zero visual leakage for our OKLch tokens, a perceptually uniform colour format, is real theming work rather than composition. It fails scoring criterion 1 outright.

**shadcn/ui** is a distribution model rather than a primitive. It is useful context, but it is not a separate scoring axis. Whichever primitive we pick, Base UI most likely, the shadcn copy-in-repo pattern is a reasonable way to receive components pre-wired to our tokens. This is provided we treat the copied code as ours to maintain. The accessibility verification obligation moves to us at that point.

**Melt UI** and **Bits UI** matter only if Svelte becomes the framework choice. Both are actively maintained, and both follow the same builder-to-componentised-layer split as Zag.js and Ark UI.

## Recommendation

There is no single winner. The recommendation is a combination, chosen to keep the framework decision open as long as possible:

- **If the framework decision needs to happen now or soon**, use React Aria Components for the full Layer 1 and Layer 2 set. It is the most complete, best-tested, zero-shipped-CSS option. It covers every primitive in scope, including Toast.
- **If we want to defer the framework decision further**, build the Overlay, Dialog, Popover, Toast, and Select contract on **Zag.js**. Consume it through the **Ark UI** component layer. This is the only path where the behaviour work, meaning light-dismiss, focus capture and return, edge-flipping, and toast stacking, is validated once. It then ports to React, Vue, Solid, or Svelte adapters without redoing the accessibility work.
- Either way, add **Floating UI** underneath for Popover edge-flipping, if the chosen library's own positioning falls short. Zag.js and Ark UI already delegate to it. Base UI and Radix have their own equivalent.
- If React is chosen and we want shadcn's copy-in-repo convenience for velocity, start from **Base UI** instead of Radix. It is the actively-maintained lineage, and it is shadcn's own 2026 default.

## What we'd still build ourselves

- **SidePanel**, the pin, dock, resize, and persist panel primitive. No surveyed library owns this. It is Overlay plus our own resize, pin, and persistence logic, already scoped as ours in `components.md` Layer 1.
- **ScreenHost**, the crossfade-on-switch behaviour for rule 5.1.2. None of these libraries model whole-screen transitions, only overlay-level ones.
- **MotionViewport**, the pan, zoom, and text-readability-floor behaviour for rule 5.7. This is canvas-specific, and it is out of scope for every candidate surveyed here.
- **Toast stacking and grouping policy**, open per `DESIGN.md`, the product design specification, §7.1. React Aria Components' toast-queue primitive and Base UI's Toast Manager both give a queue primitive. The policy to dismiss individually and as a group is our decision to encode on top.
- **FloatingOrb**, a draggable ambient assistant button. None of the surveyed libraries model a draggable floating trigger. It will likely be composed from a plain draggable behaviour plus any library's Popover for its menu.
- **Voice input on every TextInput**, the text-entry primitive, for rule 5.4.2. None of these libraries touch speech input. This stays entirely ours, layered on top of whichever TextInput primitive we adopt.
- **Container-query-driven density modes**: full, medium, and compact. All candidates are silent on this by design, since they ship no layout CSS. That is good, because it means nothing fights our container queries. The mode system itself is ours to build.

## Sources

- [react-aria-components - npm](https://www.npmjs.com/package/react-aria-components)
- [v1.20.0 | React Aria, Adobe](https://react-aria.adobe.com/releases/v1-20-0)
- [Releases, adobe/react-spectrum](https://github.com/adobe/react-spectrum/releases)
- [shadcn vs Radix vs Base UI: Which One Should a Junior Pick in 2026?, DEV Community](https://dev.to/edriso/shadcn-vs-radix-vs-base-ui-which-one-should-a-junior-pick-in-2026-1jml)
- [Radix vs Base UI: which headless React library should you use in 2026?, ShadcnDeck, a blog](https://www.shadcndeck.com/blog/radix-vs-base-ui)
- [July 2026, Base UI as the Default, shadcn/ui](https://ui.shadcn.com/docs/changelog/2026-07-base-ui-default)
- [February 2026, Blocks for Radix and Base UI, shadcn/ui](https://ui.shadcn.com/docs/changelog/2026-02-blocks)
- [Migrate from Radix UI to Base UI in 9 Easy Steps, shadcnstudio](https://shadcnstudio.com/blog/migrate-from-radix-ui-to-base-ui/)
- [GitHub, chakra-ui/ark](https://github.com/chakra-ui/ark)
- [About Ark UI | Ark UI](https://ark-ui.com/docs/overview/about)
- [Introducing Ark UI Svelte](https://ark-ui.com/blog/introducing-ark-ui-svelte)
- [GitHub, chakra-ui/zag](https://github.com/chakra-ui/zag)
- [flip | Floating UI](https://floating-ui.com/docs/flip)
- [Why CSS Anchor Positioning and the Popover API Matter in 2026](https://kvassiliou.com/tech/css-anchor-positioning-popover-api-2026)
- [tailwindlabs/headlessui, Issues](https://github.com/tailwindlabs/headlessui/issues)
- [Releases, tailwindlabs/headlessui](https://github.com/tailwindlabs/headlessui/releases)
- [WorkOS raises $80m in Series B financing, acquires Modulz, WorkOS](https://workos.com/blog/series-b)
- [Is Your Shadcn UI Project at Risk? A Deep Dive into Radix's Future](https://dev.to/mashuktamim/is-your-shadcn-ui-project-at-risk-a-deep-dive-into-radixs-future-45ei), a DEV Community post
- [GitHub, shoelace-style/shoelace](https://github.com/shoelace-style/shoelace)
- [GitHub, shoelace-style/webawesome](https://github.com/shoelace-style/webawesome)
- [webawesome/LICENSE.md at next](https://github.com/shoelace-style/webawesome/blob/next/LICENSE.md)
- [GitHub, melt-ui/melt-ui](https://github.com/melt-ui/melt-ui)
- [bits-ui - npm](https://www.npmjs.com/package/bits-ui)
- [Top Headless UI libraries for React in 2026, greatfrontend](https://www.greatfrontend.com/blog/top-headless-ui-libraries-for-react-in-2026)
