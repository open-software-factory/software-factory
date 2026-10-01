# Lessons from agent-built operator consoles

Status: research note

Date: 2026-09-05

## Where the evidence comes from

Between 21 and 24 August 2026 four coding-agent sessions each built a working React operator console for one incident scenario. Two used DeepSeek V4 Flash and two used GPT-5.6 Luna Max, each once with and once without an external design workflow. All four passed every deterministic gate: lint, format, unit tests, build, Playwright interaction checks, axe accessibility, keyboard paths, overflow and reduced motion.

On 24 August a local forensic review and an independent blind review by GPT-5.6 Sol High examined the outputs. They agreed on the causes. This note keeps what those reviews found, because the benchmark that produced them is dropped and its branches will go.

Screenshots of the four consoles are in [`prototypes/`](prototypes/README.md).

## What went wrong

**Doctrine became product copy.** The agents were given the UX vision and design principles as context. They printed phrases from those documents as UI labels. Examples seen on screen: "persistent spatial backbone", "calm is an explicit state", "every record inspectable". A design principle is an instruction to the builder. It is not a label for the operator.

**Test machinery became product UI.** The harness needed a way to step through the scenario. The agents put "scenario step", "frame mode" and "advance event" controls into the operator's header, because the tests looked for them there. Whatever the tests require in the DOM, the agent builds into the product.

**The data made "calm" impossible.** The fixture handed the UI the whole story at once. The calm view therefore showed failed checks, a stale runner and rolled-back deployments while the prose said everything was calm. The agents resolved the contradiction by asserting the principle in text.

**The decision was already made.** The scenario asked the operator to choose between rollback and forward fix, but the fixture said rollback had already happened. The decision panel looked fine and meant nothing.

**Green gates hid all of it.** Every deterministic check passed on every run. The gates proved wiring and operability. They said nothing about whether the content was true, usable or in the operator's language.

**The design workflow did not catch it.** The lane that ran explicit accessibility, hierarchy, interaction-state and anti-slop reviews still shipped the leaked doctrine.

**Operator feedback made it worse.** Revision feedback that reused internal terms was copied back into the UI.

## What this means for the Factory

These are the transferable lessons. Each one is a candidate rule awaiting a decision.

1. **Separate builder context from operator vocabulary.** Give agents the doctrine, and also give them the persona and the words the operator actually uses. State plainly that doctrine phrases must not appear on screen.
2. **Keep evaluator and test controls out of product surfaces.** Drive test states through query parameters or a test-only interface. Enforce the boundary with a source-level test so product code cannot import it.
3. **Give the UI only what is true now.** A snapshot must contain no future events. Calm means calm in the data as well as in the copy. See [`../../architecture/draft-factory-vocabulary.md`](../../architecture/draft-factory-vocabulary.md).
4. **Every offered decision must be genuinely open.** Both branches need real, testable consequences.
5. **Add semantic gates next to mechanical ones.** Assert facts about state, not only about the DOM: calm has zero critical attention and no recovery controls; both decision options and their four dimensions are visible in the initial viewport; the intent-to-outcome chain is complete; evidence actions open evidence.
6. **Scan visible copy for prohibited terms.** A deterministic check that fails on known evaluator and doctrine phrases in visible text and accessible names catches the class of error mechanically. It does not replace human review of clarity.
7. **Treat passing deterministic gates as necessary and still short of sufficient.** This is a concrete case for the open question about how deterministic gates and review signals compose. The deterministic result stays authoritative for its scope, and its scope was narrower than the requirement.
8. **Score process cost as an outcome.** The workflow lane used materially more time and tokens. Operator corrections, retries and elapsed time all count as results.

## What was sound and is worth reusing

- A concrete operator job with a real intervention sequence produced real provider-backed interaction. Static screenshots would have shown none of it.
- A fixture with stable IDs and enough connected entities supported serious traceability work.
- A separate contract surface, distinct from the product surface, was the right shape.
- Converting an operator complaint into a deterministic acceptance check worked. The "decision is below the fold" complaint became a geometry assertion.
- Comparing a local review against a blind independent review found three material problems the first review under-weighted. It is a cheap pattern for any future design review.

## Toolchain that worked

Pinned and proven on Windows 11 in August 2026. Record here as evidence only.

| Tool | Version | Note |
|---|---|---|
| Node.js | 26.7.0 | Pinned in `.nvmrc`, run through fnm |
| npm | 11.19.0 | Bundled with the Node version |
| TypeScript | 7.0.2 | |
| Vite | 8.2.1 | |
| Vitest | 4.1.10 | |
| React | 19.2.8 | |
| Zod | 4.4.3 | Schema validation for the fixture |
| Oxlint | 1.76.0 | Lint gate. No ESLint stack. |
| Oxfmt | 0.61.0 | Format gate, separate from lint |
| Playwright | 1.62.1 | With axe-core for accessibility |

Known quirk: the Playwright web server does not shut down cleanly on Windows after the final summary. The fix that worked was to stop the process by its port. Never kill Node processes broadly. One agent session did and killed its own controller.

Also known: a 31 GB npm cache filled the disk during the runs. Watch cache size on long agent sessions.

## Sources

This note summarizes a local forensic review and an independent blind review of the four console builds, both run on 24 August 2026. The review files themselves are not included in this repository. The screenshots in [`prototypes/`](prototypes/README.md) and the findings this note records are what remains of that work.
