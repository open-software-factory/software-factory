# Survey 07: the floating orb, draggable trigger, radial menu, flick physics

This survey covers `FloatingOrb`, a layer 1 component in components.md, and the menu it opens.
The orb is a circular overlay. A user can drag it anywhere on screen. A press opens one of
three things. It opens a multi-level radial menu, a regular menu, or a floating panel. The
panel reuses the non-modal Overlay the inspector already floats in. The orb has a drop
shadow and a subtle pressed feel. Radial items radiate out smoothly, and the orb stays above
everything else on screen. An optional flick gesture adds deceleration and a realistic bounce
off the viewport edge. Evidence comes from web search and package registry reads, dated 12
September 2026.

## Candidates

| Candidate | What it gives | Keyboard / ARIA | Styling | Dependencies · licence · health | Verdict |
|---|---|---|---|---|---|
| **@spaceymonk/react-radial-menu** 2.1.0 | DOM radial menu with `SubMenu` back-navigation, fade/scale/rotate animations, `innerRadius`/`outerRadius`, background drawing | not documented, mouse-first | CSS variables, dark mode via overrides | React ≥16.8 only · MIT · last publish Oct 2025 | Closest to the radial shape we want. It has no keyboard or ARIA story. We would need a fork to meet §5.4.1. |
| **@crocogiciel/react-web-radial-menu** 1.0.6 | Multi-level radial menu, TypeScript | not documented | its own visuals | peer-depends on `lucide-react`, an icon set we do not use · MIT · Feb 2026 · repository returned 404 on fetch | Ties us to a second icon system for one menu. The repository is unreachable. Do not adopt it. |
| **react-planet** 1.0.1 | "Orbit" menu: satellites around a centre | none | Material-UI 4 styles, a React UI kit | drags in react-spring 8, Material-UI 4, react-use-gesture 7 · MIT · last publish May 2022 | Abandoned dependency tree. Not a candidate. |
| **react-radial** (modelab) | SVG donut radial | none | props for stroke/fill | Resonance, a React and D3 bridge · no licence shown | Decorative and unlicensed. Not a candidate. |
| **Motion for React** (Framer Motion's successor), imported as `motion/react` | `drag` with momentum: inertia on release from pointer velocity, `dragConstraints` (object or ref), `dragTransition` `bounceStiffness`/`bounceDamping`, `dragElastic`, `whileDrag`, `onDragEnd` velocity | n/a (pointer motion only) | n/a | React only · MIT · actively released · roughly 30 to 40 kB for the drag and animate path | Would give drag, momentum, and a *spring* bounce at the constraint in one prop. The bounce is a per-axis spring against the wall. It feels soft and springy, unlike a billiard-ball reflection. Its FAB example, with staggered spring items, is paywalled source. This is a heavy dependency for one gesture the shell already does by hand for floats and drags. |
| **@use-gesture/react** 10.3.1 + **@react-spring/web** 10.1.2 | `useDrag` with velocity and `bounds`. Spring `decay` gives momentum, with `inertia` easing and `rubberband` | n/a | n/a | React ≥16.8 · MIT · use-gesture last publish Dec 2022, react-spring Dec 2023 | Two libraries, both quiet for years, to replace about 80 lines of arithmetic we can test in Node. |
| **React Aria Components** (already adopted) | `Menu`/`MenuItem` semantics and keyboard. `Popover` with `triggerRef` gives an anchored regular menu. The non-modal `Overlay` gives the panel | ✅ | ours | in the bundle already | The semantics layer for every menu the orb opens. The radial *layout* is CSS on top of a `Menu`. |

## Findings

1. **No library combines both.** No shipped component offers a draggable trigger that opens a
   radial menu. Every radial menu assumes a fixed anchor. Every drag library stops at the
   gesture. Survey 01 found the same thing. We compose the orb from a drag behaviour plus a
   library popover.
2. **Radial menus are mouse-first.** None of the surveyed radial menus documents arrow-key
   navigation or menu roles. This turns §5.4.1 from a nice-to-have into a blocker. React Aria,
   the accessible component library we already use, solves this. Its `Menu` component keeps
   arrow keys, Home and End, typeahead, and roles working when laid out on a circle. The ring
   itself is only `transform: rotate(θ) translate(r) rotate(−θ)` per item.
3. **Physics is small enough to own.** Momentum uses exponential decay. A wall reflection
   reverses the normal velocity component, keeps the tangential one, and scales the result by
   a restitution value. Together these form a pure function of position, velocity, and time.
   Owning this code means we can unit-test it in Node, the same way we test the placement
   reducer. The owner asked for a bounce that matches the angle it hit at. That is exactly a
   reflection. The library springs do not produce a reflection.
4. **Panel mode is already built.** The non-modal Overlay is the floating panel, the same one
   the inspector floats in. The orb only chooses what goes inside it, mini chat or voice.

## Recommendation

Adopt nothing new. Build `FloatingOrb` in its own file on the primitives we have:

- `packages/console-model/src/flick.ts` holds the physics as data. It tracks velocity from the last
  pointer samples, decay per millisecond, reflection at the bounds with restitution, and a
  stop threshold. A `settle()` function predicts the landing point. This file is tested.
- `packages/console-ui/src/orb.ts` is the component. Pointer capture handles drag. A press under 6 px
  opens the orb. A release with velocity runs the flick on `requestAnimationFrame` through the
  model. It has three open modes. The **radial** mode uses a React Aria `Menu` on a ring, with
  submenus as a second ring that has a back item. The **menu** mode uses a React Aria
  `Popover` anchored to the orb via `triggerRef`. The **panel** mode uses the shared non-modal
  Overlay, with content supplied by the host, such as mini chat or voice.
- `apps/console-lab/orb.html` is a lab page. It lets us try the variants over real
  content before the orb is wired into the shell. Variants switch from the lab bar. The lab
  bar is scaffolding. It does not ship as part of the product.

Revisit Motion for React only if the shell later needs a general animation layer. For the orb
alone, it is a large dependency for a small, testable piece of arithmetic.

## Sources

- [@spaceymonk/react-radial-menu on npm](https://www.npmjs.com/package/@spaceymonk/react-radial-menu) · [GitHub](https://github.com/spaceymonk/react-radial-menu)
- [@crocogiciel/react-web-radial-menu on npm](https://www.npmjs.com/package/@crocogiciel/react-web-radial-menu)
- [react-planet on npm](https://www.npmjs.com/package/react-planet)
- [modelab/react-radial on GitHub](https://github.com/modelab/react-radial)
- [React drag animation guide from Motion for React](https://motion.dev/docs/react-drag) · [Floating Action Button example](https://motion.dev/examples/react-floating-action-button)
- [@use-gesture documentation](https://use-gesture.netlify.app/docs/) · [@use-gesture/react on npm](https://www.npmjs.com/package/@use-gesture/react) · [@react-spring/web on npm](https://www.npmjs.com/package/@react-spring/web)
