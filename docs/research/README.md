# Research Index

The files in this directory are working research notes. They are not canonical requirements. Re-check current upstream projects before making decisions because this tooling landscape changes quickly.

## Topics already explored in this project

- `reusable-skills.md`: existing workflow/skill systems and the implication that the factory should compose rather than recreate them.
- `openhands-and-agent-platforms.md`: OpenHands and adjacent agent platforms as comparison points.
- `build-runners-and-compute.md`: multi-OS/architecture runners, GitHub Actions/self-hosting and external compute-provider considerations.
- `build-systems-and-remote-caching.md`: Moonrepo and the question of where builds/caching belong.
- `ai-pr-walkthroughs.md`: intelligent human-facing PR/code-understanding systems for AI-generated changes.
- `ahp-acp-architecture-direction.md`: approved direction for treating ACP and AHP as first-class protocol edges while keeping factory lifecycle and orchestration semantics independent.
- `ux/design-workflow-landscape.md`: candidate AI-assisted design workflows and a proposed benchmark with non-designer usability as a hard gate.
- `ux/benchmark/benchmark-design.md`: the controlled UX workflow benchmark. Built and run four times in August 2026, then dropped. Its reusable findings are in the two entries below.
- `ux/agent-built-ui-lessons.md`: what went wrong when coding agents built operator consoles from the UX doctrine, what transfers to the factory, and the toolchain that worked.
- `ux/prototypes/README.md`: screenshots of the four agent-built operator console prototypes, with the caveats needed to read them.
- `ux/ux-references.md`: interface references and the specific qualities worth studying in each.
- `2026-09-15-skill-lint-validation-and-evaluation.md`: what validation, testing and evaluation of agent skills looks like in the wild, and why a lint alone is not enough.
- `2026-09-18-moon-as-osf-execution-substrate.md`: why moon is adopted as the execution engine for deterministic tasks, and what stays outside it.
- `2026-09-17-change-risk-classification.md`: what research, industry systems, deterministic tooling, models and regulated domains say about classifying a change set's risk, and what that changes in the classifier design.
- `2026-09-25-verifier-and-reducer-review-patterns.md`: a public coding-agent project that scores a change with several independent model reviews, then checks the result with a deterministic reducer, compared against the factory's own review-lens and deterministic-authority records.
- `2026-09-25-qlty-as-a-check-runner.md`: what qlty checks by ecosystem, its licence terms, and how its own default picks compare with a per-ecosystem tool list.
- `2026-09-25-native-tools-per-verification-slot.md`: one fast, native tool per verification slot, per ecosystem, plus a deeper look at Rust architecture tools and .NET licence-audit tools.

These notes capture questions, conclusions and leads from earlier research. Refresh them from current primary sources when a topic becomes implementation-critical.
