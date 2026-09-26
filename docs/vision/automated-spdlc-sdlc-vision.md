# Automated SPDLC / SDLC

A vision for automating software and product delivery.

Status: living draft

This document describes an aspirational direction for end-to-end automation of the product and software-delivery lifecycle. Some parts are not achievable yet and will change as LLMs and deterministic tools improve. The goal is to remove tedious, low-judgment work and concentrate human attention on direction, taste and decisions.

## 0. Engineering principles

Software engineering principles describe properties of systems that must be changed, verified and operated over time. An agent-native world changes who or what reads, writes and reasons about code, while the underlying engineering challenge remains.

This section establishes the document's invariants. Tools, gates and thresholds will evolve as capabilities advance; the principles should remain stable. Their agent-native justification guides decisions when the tooling changes.

### 0.1 Principles that survive unchanged

These were never about human comprehension. Their justification was always about system properties, so the shift to agent execution changes nothing about them.

| Principle                       | Why It Has Always Mattered                                                                                            | Why It Still Matters                                                                                                                                                                 |
|-------------------------------------|---------------------------------------------------------------------------------------------------------------------------|------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| Separation of concerns / modularity | Bounded scope reduces the blast radius of change and the surface of what must be understood to make a change safely       | Agent tasks are bounded. Fitness functions check boundaries. Smaller, isolated units have smaller verification surface and cleaner failure modes.                                        |
| Single responsibility               | A unit with one reason to change is easier to test, understand, and modify without unintended consequences                | Agent scope is clear and independently verifiable. When a unit does one thing, automated verification of that thing is complete and unambiguous.                                         |
| Explicit contracts and interfaces   | Teams can work independently when boundaries are precisely defined                                                        | Agents operating in parallel need unambiguous boundaries. Implicit convention breaks at scale. Explicit contracts are machine-checkable.                                                 |
| Data integrity                      | Data is the most expensive thing to recover. Schema correctness and referential integrity are non-negotiable.             | Data integrity constraints are independently verifiable. They survive the code being entirely replaced by agents.                                                                        |
| Security by design                  | Vulnerabilities are cheaper to fix early. Attack surface belongs to the system itself, and stays the same whoever reads the code.                | Automated security gates verify against a defined threat model. Bolt-on security has no specification to verify against.                                                                 |
| Resilience patterns                 | Systems that handle failure gracefully are more reliable. Failure modes should be designed up front, before production finds them. | Failure modes defined in spec become verifiable requirements. Graceful degradation can be tested deterministically in staging.                                                           |
| Reversibility                       | Mistakes can be undone. Irreversible decisions warrant proportionally more care.                                          | The entire progressive rollout and auto-rollback model rests on reversibility. Irreversible decisions are the threshold for higher human oversight, regardless of automation confidence. |

### 0.2 Principles with a changed justification

These principles survive, but their mechanism has changed. The new mechanism explains why each principle remains useful and when it might weaken.

#### Naming things well

The traditional justification: humans read code and names communicate intent. A well-named variable or function is the fastest documentation.

The agent-native justification: the L in LLM is language. Models are trained predominantly on human-readable code. Agent comprehension of code currently tracks human readability because the training data makes them closely related. “Write code an agent can understand” and “write code a human can understand” currently produce the same answer.

This equivalence depends on training data rather than anything fundamental about agents. Most existing and foreseeable code is human-readable, so models will continue to train on it. Human readability therefore remains the best available proxy for agent readability, even if the equivalence is not permanent.

#### DRY: don't repeat yourself

The traditional justification: two copies of the same logic maintained by humans inevitably diverge. One gets updated, one does not. Inconsistency is a function of human fallibility and attention limits.

The agent-native justification: duplicate logic gives an agent inconsistent context about which version is canonical, which to update and which represents current intent. The failure mode shifts from human inconsistency to agent ambiguity, with the same conclusion: do not duplicate.

#### SOLID principles

Each of the five SOLID principles survives, with justification that is at least as strong in an agent-native context:

- Single Responsibility: agent task scope is clean and independently verifiable

- Open/Closed: agent extensions work against a stable interface without requiring full understanding of the implementation

- Liskov Substitution: substitutability is machine-checkable; violations break automated contract tests

- Interface Segregation: narrow interfaces reduce the context an agent needs to implement against correctly

- Dependency Inversion: agents work against contracts, not implementations; swapping implementations does not require understanding the consumers

#### Consistency across the codebase

The traditional justification: new team members learn the codebase through consistent patterns. Inconsistency forces each person to hold multiple mental models simultaneously.

The agent-native justification: an inconsistent codebase gives an agent conflicting signals about what correct looks like. When it sees three implementations of the same pattern, it may average across them and produce a fourth. Consistency helps an agent build an accurate model of what the codebase considers correct.

### 0.3 Previously aspirational practices that become achievable

These established practices were consistently underfunded because of cost. Engineers knew that comprehensive tests, thorough instrumentation and full traceability were valuable, but cut them under delivery pressure when human time was scarce.

In an agent-native world, agents write tests, generate instrumentation and maintain traceability. The lower time cost allows these practices to become the baseline.

| Practice                       | Always Known to Be Valuable                                                                                                                      | Why It Was Underinvested                                                                                                                                   | Why It Is Now Achievable                                                                                                 |
|------------------------------------|------------------------------------------------------------------------------------------------------------------------------------------------------|----------------------------------------------------------------------------------------------------------------------------------------------------------------|------------------------------------------------------------------------------------------------------------------------------|
| Comprehensive verifiability        | A property needs a runnable check before it can be guaranteed. Every meaningful system property should be expressed this way.                       | Writing tests well enough to catch logic errors takes significant time. Mutation, contract and boundary tests are expensive to maintain.                         | Agents generate tests, reducing test-authorship cost and allowing a higher bar.                                               |
| Instrumentation density            | You cannot reason about a system you cannot observe. Comprehensive tracing, logging, and metrics were always the right default.                      | Instrumentation was the first thing cut in delivery pressure. Adding it properly across a whole system is tedious, time-consuming work.                        | Agents instrument as they build. Observability requirements defined in the spec are verified present by the pipeline.        |
| Spec-to-code traceability          | Every significant structure in a codebase should be traceable to an intent decision. Code whose purpose is unclear is a liability.                   | Maintaining traceability as the codebase grew was a documentation burden that fell behind delivery.                                                            | Agents produce code in direct response to specs. Traceability falls out of the process itself, so it needs no separate maintenance. |
| Complete error handling            | Every failure path should be handled explicitly. Silent failures and swallowed exceptions are bugs waiting to manifest.                              | Exhaustive error handling is tedious to write and easy to deprioritise in a working happy-path implementation.                                                 | Agents can be instructed to handle all error paths. SAST rules verify completeness. The pipeline catches gaps.               |
| Architectural documentation (ADRs) | Every significant architectural decision should be documented with its rationale and alternatives considered.                                        | Writing ADRs was a discipline that required time and was easy to skip under pressure.                                                                          | Agents generate ADR drafts. The requirement to link specs to ADRs is enforced by the pipeline.                               |

Agent execution lowers the cost barrier to following these established practices.

### 0.4 What changes: the implementation layer

The engineering principles this document opens with are stable. They do not change with the tooling.

The specific tools, gates, pass/fail thresholds and balance between automated and human verification will continue to change. The rest of this living document covers that implementation layer.

When a better SAST tool emerges, replace the current one. When an LLM can reliably detect a class of architectural problem that previously required a fitness function, retire the fitness function. The implementation layer is the part that tracks the frontier. The principles section is the part that tells you what you are always trying to achieve, regardless of how you are achieving it.

## 1. Framing the vision

### 1.1 The goal

The goal is to eliminate low-judgment, repetitive and tedious work, leaving people to exercise taste, direction and judgment where there is no objectively correct answer.

The system should extend human decisions across repeated work. One decision, such as “set the complexity threshold to 15” or “our APIs follow REST rather than RPC,” can govern thousands of future checks. People set values, thresholds and direction; the system operates within those parameters until it encounters an exception.

### 1.2 Human input at maturity

| Category       | Description                                                                                                                        | Automation Ceiling                                                      |
|--------------------|----------------------------------------------------------------------------------------------------------------------------------------|-----------------------------------------------------------------------------|
| Direction          | Feature priorities, business goals, product vision. Requires understanding of users, market, strategy.                                 | AI surfaces options, models tradeoffs and flags contradictions; human decides |
| Values & Tradeoffs | Speed vs stability, innovation vs consistency, complexity vs simplicity. Policy decisions codified once, automated against thereafter. | Fully automated once policy is set                                          |
| Taste              | UX feel, visual design, code elegance, tone of voice. Genuinely hard to fully automate.                                                | Captured in standards; automation enforces standards, flags deviations      |
| Calibration        | Tuning the automation itself: are gates set right? Are LLM prompts optimal? Is the process producing the right outcomes?               | Humans review the system rather than every output (meta-loop)               |
| Exception Handling | When the system signals genuine ambiguity, low confidence, novel situation, ethical/legal edge case.                                   | System escalates; human responds. Everything else runs.                     |

### 1.3 Design principles

- Only exceptions bubble up. If a human reviews routine output, the system is not automated enough yet.

- Human input should have broad reach. One policy decision governs thousands of automated checks.

- Confidence is a first-class output. Every automated step produces a result and a confidence signal. Low confidence triggers escalation; high confidence allows progress.

- The system explains itself. Automated decisions include rationale. Humans spot-checking can audit without re-doing the work.

- Reversibility over perfection. The more reversible a decision, the more aggressively it can be automated. Irreversible decisions warrant higher oversight regardless of confidence.

- The spec is the source of truth. All automation traces back to intent in specs and ADRs. When automation disagrees with human expectation, ask whether the spec is right.

- Deterministic gates are authoritative. Security, quality and compliance checks that can run as tools must be hard CI gates rather than LLM prompts.

### 1.4 Trust calibration

Automation is not switched on all at once. Each domain earns the right to reduced oversight based on demonstrated reliability:

| Level        | Description                                         | When to Use                                               |
|------------------|---------------------------------------------------------|---------------------------------------------------------------|
| High oversight   | Human reviews every output                              | New automation, unproven domain, high blast radius            |
| Spot-check       | Human samples a percentage of outputs                   | Established automation, low false-positive rate               |
| Exception-only   | Human responds to escalations only                      | High-confidence automation, tracked reliability metrics       |
| Fully autonomous | Human reviews the aggregate metrics and never an individual output | Mature, stable, monitored automation with rollback capability |

Track false-positive rates, escaped defects and near-misses per domain. Use the data to recommend where oversight should loosen or tighten. Phase 11 (the meta-loop) defines the periodic review for this work.

## 2. Full SPDLC / SDLC map

The complete lifecycle spans twelve phases, from discovery through to the meta-loop that reviews the process itself. Each phase has automation/verification dimensions, triggers, and frequency targets.

| Phase | Name                     | Domain               | Primary Concern                                                                      |
|-----------|------------------------------|--------------------------|------------------------------------------------------------------------------------------|
| 0         | Discovery & Ideation         | Product                  | Is this worth building? What is the opportunity?                                         |
| 1         | Requirements & Specification | Product + Engineering    | Is it complete, unambiguous, testable, and compliant?                                    |
| 2         | Design & Architecture        | Engineering              | Does the design solve the spec correctly, safely, and sustainably?                       |
| 3         | Implementation               | Engineering              | Is the code correct, quality, tested, and observable?                                    |
| 4         | Code Review & PR             | Engineering              | Does this change meet quality, traceability, and safety bars?                            |
| 5         | CI Pipeline                  | Engineering + Security   | Do automated gates confirm quality, security, and supply chain integrity?                |
| 6         | Staging / Pre-Production     | Engineering + QA         | Does the system behave correctly, performantly, and securely under realistic conditions? |
| 7         | Production Deployment        | Engineering + Operations | Does the deployment proceed safely with rollback capability?                             |
| 8         | Post-Deployment Operations   | Operations + Security    | Is the system reliable, secure, and cost-effective in production?                        |
| 9         | Feedback Loops               | All                      | Are findings from production informing upstream phases?                                  |
| 10        | Cross-Cutting Concerns       | All                      | Security, observability, privacy, compliance and cost across every phase                 |
| 11        | The Meta-Loop                | Process                  | Is the SDLC process itself still optimal?                                                |

### Phase 0: Discovery and ideation

Domain: Product. Upstream of the spec, this phase asks whether the work is worth building.

#### Dimensions and verification

| Dimension / Check | What is Verified                                                        | Trigger      | Frequency      |
|-----------------------|-----------------------------------------------------------------------------|------------------|--------------------|
| Business value        | ROI signal, strategic alignment with current OKRs/roadmap                   | New idea/request | Per request        |
| Duplication detection | Does this overlap with existing features or in-progress work?               | New idea/request | Per request        |
| Feasibility signal    | Rough complexity estimate before any spec is written                        | New idea/request | Per request        |
| Dependency mapping    | What existing systems/features does this touch?                             | New idea/request | Per request        |
| Impact radius         | How many users/systems affected? Regression risk?                           | New idea/request | Per request        |
| Reversibility         | How easy is this to undo if the direction is wrong?                         | Design review    | Per proposal       |
| Opportunity cost      | What is not being built because of this? Explicit trade-off.                | Prioritisation   | Per sprint/cycle   |
| User signal           | Usage analytics, support tickets, user research feeding back into discovery | Continuous       | Weekly aggregation |

Most SDLC automation assumes requirements already exist, leaving discovery underserved by current agentic frameworks. AI can assist with synthesis, duplication detection and trade-off modelling, while people remain responsible for what should be built and why.

### Phase 1: Requirements and specification

Domain: Product and engineering. This phase is the main gate into the pipeline.

#### Quality dimensions of a specification

Downstream automation cannot repair an underspecified or ambiguous spec. The spec-quality gate affects every later phase.

| Dimension / Check         | What is Verified                                                             | Trigger                              | Frequency                    |
|-------------------------------|----------------------------------------------------------------------------------|------------------------------------------|----------------------------------|
| Completeness                  | All scenarios covered: happy path, edge cases, error states, rollback            | On spec creation/update                  | Every edit                       |
| Ambiguity                     | Precise enough that acceptance criteria can be derived without interpretation    | On spec creation/update                  | Every edit                       |
| Consistency                   | No contradictions within this spec or against existing specs                     | On spec creation/update; cross-spec scan | Every edit + nightly cross-check |
| Testability                   | Every requirement can become a failing test before implementation                | Before design begins                     | Pre-design gate                  |
| Traceability                  | Links to business goal, OKR, user need, or ADR                                   | On spec creation                         | Every edit                       |
| Feasibility                   | Technically achievable in current architecture, or architecture change is scoped | Design review                            | Per spec                         |
| Non-functional requirements   | Performance SLOs, security requirements, accessibility, privacy, reliability     | On spec creation                         | Every edit                       |
| Acceptance criteria           | Explicit, measurable definition of done; binary pass/fail                        | Before development begins                | Pre-dev gate                     |
| Migration / rollout strategy  | How this deploys without breaking existing users; feature flag plan              | Before design                            | Per spec                         |
| Privacy / data classification | What PII is involved? Retention, deletion, consent requirements                  | On spec creation                         | Every edit                       |
| Compliance requirements       | Regulatory constraints: GDPR, PCI, SOC2, accessibility law, etc.                 | On spec creation                         | Every edit                       |
| Observability requirements    | What must be logged, traced and alerted on, defined in the spec                  | On spec creation                         | Every edit                       |
| Dependency declaration        | What other specs/features must be complete first?                                | On spec creation                         | Every edit                       |

Specifications are living documents and may contain ambiguity as the product and business evolve. Automation validates the current content against these dimensions and makes gaps visible.

### Phase 2: Design and architecture

Domain: Engineering. This phase addresses correctness, sustainability and security at the structural level.

#### Dimensions and verification

| Dimension / Check  | What is Verified                                                            | Trigger                            | Frequency                                  |
|------------------------|---------------------------------------------------------------------------------|----------------------------------------|------------------------------------------------|
| Correctness            | Does the design address every scenario in the spec?                             | On design creation/update              | Every edit                                     |
| Completeness           | No spec scenario left unaddressed in the design                                 | Design review                          | Per design                                     |
| Pattern consistency    | Follows established architectural patterns; deviations documented as ADRs       | Design review                          | Per design                                     |
| Simplicity (YAGNI)     | Simplest solution that works; no speculative abstraction                        | Design review                          | Per design                                     |
| Extensibility          | Accommodates known future directions without over-engineering                   | Quarterly design review                | Quarterly                                      |
| Separation of concerns | Clean layer boundaries; single responsibility at system level                   | Design review + dependency analysis    | Per design + weekly scan                       |
| Data model integrity   | Schema design, normalisation, migration path, backwards compatibility           | Schema review                          | Per schema change                              |
| API design quality     | Contract clarity, versioning strategy, breaking change awareness, OpenAPI spec  | On API creation/change                 | Per API change                                 |
| API contract tests     | Consumer-driven contracts (Pact) generated from design before implementation    | Before implementation begins           | Per API design                                 |
| Observability design   | Logging, tracing, metrics designed in; SLIs/SLOs defined here                   | Design review                          | Per design                                     |
| Resilience design      | Failure modes, circuit breakers, retry strategies, graceful degradation defined | Design review                          | Per design                                     |
| Threat model           | STRIDE analysis; trust boundaries; data flows; attack surface changes           | Design review; on spec security change | Per design + on security-relevant spec changes |
| Performance design     | Expected load, bottlenecks identified, SLOs defined before implementation       | Design review                          | Per design                                     |
| Cost design            | Expected cloud cost profile; cost per operation estimated                       | Design review                          | Per design; monthly actuals review             |
| Tech choice review     | Are the chosen language/framework/libraries still appropriate?                  | Major version releases; quarterly      | Quarterly                                      |
| Architecture drift     | Is the codebase drifting from the intended architecture over time?              | Weekly automated scan                  | Weekly                                         |

#### Artefacts produced

- ADR (Architecture Decision Record) for every significant design decision, linked to the spec

- OpenAPI specification generated before implementation

- Threat-model document with STRIDE analysis, linked to the ADR

- SLO/SLI definitions expressed as configuration or code and committed to the repository

- Data-migration plan for any schema change, including a rollback procedure

- Cost estimate documented in the ADR

### Phase 3: Implementation

Domain: Engineering. This phase addresses correctness, quality, testability and observability in code.

#### Code-quality dimensions

| Dimension / Check        | What is Verified                                                      | Trigger           | Frequency           |
|------------------------------|---------------------------------------------------------------------------|-----------------------|-------------------------|
| Compilation / type safety    | Strictest compiler/type-checker settings; zero warnings treated as errors | Pre-commit            | Every save / pre-commit |
| Linting                      | Language idioms, anti-patterns; zero violations at enforced ruleset       | Pre-commit            | Every save / pre-commit |
| Style / formatting           | Deterministic; auto-applied, no debate                                    | Pre-commit (auto-fix) | Every save              |
| Cyclomatic complexity        | Per-function complexity below threshold (e.g. 10)                         | Pre-commit            | Every commit            |
| Cognitive complexity         | Human-readability complexity metric; separate from cyclomatic             | Pre-commit            | Every commit            |
| Size constraints             | File, class, method, function line count limits enforced                  | Pre-commit            | Every commit            |
| SOLID adherence              | Static analysis rules for SRP, OCP, LSP, ISP, DIP violations              | CI                    | Every PR                |
| Design pattern conformance   | Does implementation match the patterns defined in design?                 | CI + weekly scan      | Every PR + weekly       |
| Code smell detection         | God classes, feature envy, data clumps, primitive obsession               | CI                    | Every PR                |
| Dependency direction         | No upward imports; layer boundaries not violated                          | Pre-commit + CI       | Every commit            |
| Dead code                    | Unreachable code detected and flagged                                     | CI                    | Every PR                |
| Duplication (DRY)            | Copy-paste detection over a set threshold                                 | CI                    | Every PR                |
| Error handling completeness  | Every error path handled explicitly; no swallowed exceptions              | CI (SAST rules)       | Every PR                |
| Logging correctness          | Right level, right content, no sensitive data logged                      | CI (SAST rules)       | Every PR                |
| Async / concurrency          | Race condition and deadlock patterns detected (partial automation)        | CI                    | Every PR                |
| Secrets in code              | No credentials, keys, or tokens in source                                 | Pre-commit (Gitleaks) | Every commit            |
| Observability implementation | Required traces, logs, metrics from spec actually present in code         | CI                    | Every PR                |
| Configuration validation     | All config values typed, validated at startup, documented                 | CI                    | Every PR                |
| Feature flag usage           | New features behind flags if progressive rollout required                 | CI (custom rule)      | Every PR                |

#### Test-quality dimensions

| Dimension / Check         | What is Verified                                                                | Trigger             | Frequency                       |
|-------------------------------|-------------------------------------------------------------------------------------|-------------------------|-------------------------------------|
| Coverage: line/branch/path    | Minimum thresholds enforced; path coverage where feasible                           | CI                      | Every PR                            |
| Mutation score                | Mutation testing to verify tests catch logic errors rather than merely execute code | CI (scheduled)          | Nightly / per PR for critical paths |
| Test naming                   | Names describe behaviour. Implementation detail stays out                                       | CI (LLM review)         | Every PR                            |
| Test independence             | No shared state; no order dependency                                                | CI                      | Every PR                            |
| Boundary / edge case coverage | Boundaries and error paths tested alongside the happy path                          | CI (LLM review)         | Every PR                            |
| Error path coverage           | Failure modes explicitly tested                                                     | CI                      | Every PR                            |
| Performance test existence    | Every critical path has a benchmark                                                 | CI                      | Every PR touching critical paths    |
| Contract test existence       | Every API integration point has a consumer contract test                            | CI                      | Every PR touching API               |
| Accessibility test existence  | Every UI component has accessibility assertions                                     | CI                      | Every PR touching UI                |
| Test-to-spec traceability     | Tests reference the acceptance criteria they verify                                 | CI (traceability check) | Every PR                            |

### Phase 4: Code review and pull request

Domain: Engineering. This phase addresses quality, traceability and safety before merge.

| Dimension / Check       | What is Verified                                                        | Trigger           | Frequency            |
|-----------------------------|-----------------------------------------------------------------------------|-----------------------|--------------------------|
| PR size                     | Hard gate: lines changed, files changed, logical scope. Large PRs blocked.  | PR open/update        | Every PR event           |
| PR description quality      | Explains why and what; links to spec/ADR; describes testing approach        | PR open/update        | Every PR event           |
| Spec / ADR traceability     | PR links to originating spec or ADR                                         | PR open               | Every PR                 |
| Breaking change declaration | Is this a breaking change? Declared explicitly with migration notes         | PR open/update        | Every PR                 |
| Migration script presence   | Schema changed? Migration script present and tested                         | PR open/update        | Every PR touching schema |
| Rollback plan               | For significant changes, rollback procedure documented                      | PR open               | Per significant change   |
| Knowledge distribution      | Is this change known to more than one person? Bus factor check              | PR open               | Every PR                 |
| Reviewer appropriateness    | Domain knowledge reviewer assigned based on files changed                   | PR open (auto-assign) | Every PR                 |
| Branch age                  | Stale branches detected and escalated                                       | Scheduled             | Daily                    |
| All CI gates passed         | Full CI suite green before review begins                                    | PR ready-for-review   | Every PR                 |

### Phase 5: CI pipeline

Domain: Engineering and security. Automated gates confirm quality, security and supply-chain integrity.

#### Fast gates (every commit, under 2 minutes)

- Compilation + type-checking at strictest settings

- Code formatting (auto-applied)

- Linting (enforced, no warnings)

- Secret detection (Gitleaks / detect-secrets)

- Unit tests

#### PR gates (every PR, under 10 minutes)

- Full test suite: unit + integration

- Code quality: complexity, size, duplication, dead code, SOLID rules

- SAST: Semgrep (OWASP ruleset) + language-specific tools (Bandit, CodeQL, etc.)

- SCA: Snyk / OWASP Dependency-Check for CVE and licence compliance

- Supply chain: Socket.dev for behaviour-based checks beyond CVEs

- IaC scan: Checkov + tfsec (all Terraform/CloudFormation/K8s manifests)

- Container scan: Trivy + Hadolint (image + Dockerfile)

- OpenAPI spec validation: Spectral with OWASP API Security ruleset

- API breaking change detection: against current consumer contracts

- SBOM generation: every build produces a Software Bill of Materials

- Bundle size budget enforcement (frontend)

- Performance regression: benchmark suite, alert on \> threshold regression

- Accessibility: axe-core / Pa11y against UI components

- Visual regression: screenshot diffing for UI changes

- Database migration test: run against production-snapshot schema, test rollback

- Licence compliance: every dependency licence against allowed list

#### Merge gate (on merge to main)

- Full suite including mutation testing on changed paths

- Architecture conformance: dependency-cruiser / import-linter against intended design

- Build reproducibility check

- SBOM committed and signed

SAST and SCA are deterministic hard CI gates. LLM security review may supplement them but cannot replace them.

### Phase 6: Staging / pre-production

Domain: Engineering, QA and security. This phase tests the system under realistic conditions before production.

| Dimension / Check      | What is Verified                                                                        | Trigger       | Frequency                     |
|----------------------------|---------------------------------------------------------------------------------------------|-------------------|-----------------------------------|
| Smoke tests                | Critical user journeys verified end-to-end                                                  | On staging deploy | Every deploy                      |
| UAT automation             | Automated flows covering acceptance criteria from spec                                      | On staging deploy | Every deploy                      |
| DAST baseline              | OWASP ZAP baseline scan against staging endpoints                                           | On staging deploy | Every deploy                      |
| Security headers           | shcheck / Mozilla Observatory: all HTTP security headers present and correct                | On staging deploy | Every deploy + nightly            |
| TLS configuration          | SSL Labs API: TLS version, cipher suite, certificate validity                               | On staging deploy | Every deploy                      |
| API contract verification  | Consumer contracts satisfied in staging environment                                         | On staging deploy | Every deploy                      |
| Load / performance testing | Realistic traffic simulation (k6 / Locust); SLO pass/fail gate                              | On staging deploy | Every deploy + nightly full suite |
| Chaos / resilience testing | Fault injection such as pod kill, network partition and latency spike; graceful degradation verified | Nightly     | Nightly                           |
| Data migration dry run     | Migration run on production data clone; integrity verified; duration timed                  | On schema change  | Per schema change                 |
| Cross-browser / device     | Playwright matrix across target browsers and viewport sizes                                 | On staging deploy | Every deploy                      |
| Accessibility audit        | Full WCAG 2.1 AA audit against staging                                                      | On staging deploy | Every deploy                      |
| Rollback rehearsal         | Automated test that rollback procedure works                                                | Nightly           | Nightly                           |
| SLO burn simulation        | Does alerting fire when SLOs are being burned?                                              | Weekly            | Weekly                            |
| Third-party integration    | All external integrations verified in staging configuration                                 | On staging deploy | Every deploy                      |

### Phase 7: Production deployment

Domain: Engineering and operations. Deployment proceeds with automated rollback capability.

| Dimension / Check             | What is Verified                                                    | Trigger            | Frequency             |
|-----------------------------------|-------------------------------------------------------------------------|------------------------|---------------------------|
| Progressive rollout gate          | Canary/blue-green with automated metrics comparison before full rollout | On deployment          | Every deployment          |
| Feature flag verification         | New features confirmed behind flags; flags verified off by default      | On deployment          | Every deployment          |
| SLO baseline capture              | Pre-deployment SLO state recorded for comparison                        | On deployment start    | Every deployment          |
| Automated rollback trigger        | Error rate, latency or SLO burn crosses threshold and triggers rollback | Continuous post-deploy | Continuous (5-min window) |
| Post-deploy smoke tests           | Critical journeys verified in production within minutes of deploy       | On deployment complete | Every deployment          |
| Security headers (prod)           | Headers re-verified in production because CDN/WAF may alter them        | On deployment complete | Every deployment          |
| Post-deploy contract verification | All API contracts satisfied in production                               | On deployment complete | Every deployment          |
| Data integrity verification       | Post-migration data counts / checksums match expected                   | On migration complete  | Per migration             |
| Deployment changelog              | Automated release notes from commits + specs; stakeholder notification  | On deployment complete | Every deployment          |

### Phase 8: Post-deployment operations

Domain: Operations and security. This phase covers reliability, security and cost in production.

| Dimension / Check           | What is Verified                                                          | Trigger | Frequency             |
|---------------------------------|-------------------------------------------------------------------------------|-------------|---------------------------|
| Observability completeness      | All defined traces, logs and metrics flowing, beyond basic uptime             | Continuous  | Continuous                |
| SLO / SLI monitoring            | Error budgets and burn-rate alerts tracked alongside uptime                   | Continuous  | Continuous                |
| Anomaly detection               | Statistical anomalies in business and technical metrics                       | Continuous  | Continuous                |
| Cost monitoring                 | Per-service, per-feature cost tracking; alert on anomalies                    | Continuous  | Daily digest + on anomaly |
| Dependency freshness            | CVEs, version lag, EOL warnings and supply-chain alerts                        | Daily       | Daily                     |
| DAST scheduled full scan        | OWASP ZAP active scan against production                                      | Scheduled   | Weekly                    |
| Infra posture                   | Prowler / ScoutSuite cloud security posture against CIS benchmarks            | Scheduled   | Weekly                    |
| Certificate expiry              | All TLS certificates checked with advance warning                             | Scheduled   | Daily                     |
| Runtime security                | Falco / eBPF anomaly detection in containers                                  | Continuous  | Continuous                |
| Privacy / compliance monitoring | PII flowing where it should not; retention violations                         | Continuous  | Continuous                |
| Technical debt measurement      | Tracked as a trend over time rather than only at a point in time              | Scheduled   | Weekly trend report       |
| Feature usage analytics         | Is the built feature being used? Feeds back to discovery.                     | Continuous  | Weekly digest             |
| Incident capture                | Incidents auto-generate postmortem drafts linked to affected spec/ADR         | On incident | Per incident              |

### Phase 9: Feedback loops

Findings from production must inform upstream phases.

#### Critical loops

Most SDLC automation moves from spec to code to deployment. Feedback into upstream phases allows the system to improve over time.

| Loop                        | Trigger                                     | What Updates                                                                                           | Automation Level                            |
|---------------------------------|-------------------------------------------------|------------------------------------------------------------------------------------------------------------|-------------------------------------------------|
| Incident → Spec/Design          | Incident resolved, postmortem complete          | Spec (was the requirement wrong?) or ADR (was the design wrong?). Regression test generated automatically. | Semi-automated: draft generated, human approves |
| Security finding → Spec         | DAST/pentest finding closed                     | Spec for remediation rather than only a ticket. Pattern codified as new SAST rule.                          | Semi-automated                                  |
| Feature usage → Discovery       | Weekly analytics digest                         | Unused features flagged for removal. Usage data informs SLO definitions.                                   | Automated: human acts on digest                 |
| Test failure → Spec             | Test fails due to spec ambiguity rather than a code bug | Spec and test updated so the ambiguity is resolved at its source.                                  | LLM-assisted: proposes spec clarification       |
| Tech debt trend → Roadmap       | Weekly debt trend crosses threshold             | Debt paydown scheduled as first-class work item alongside features                                         | Automated report; human schedules               |
| Performance regression → Design | Benchmark regression detected in CI or staging  | Design review triggered for affected components                                                            | Automated trigger; human reviews design         |
| Cost anomaly → Design           | Cloud cost exceeds per-operation budget         | Cost design review triggered for affected services                                                         | Automated trigger; human reviews                |

### Phase 10: Cross-cutting concerns

These concerns apply throughout the lifecycle.

#### Security at every phase

| Dimension / Check   | What is Verified                                                                         | Trigger             | Frequency          |
|-------------------------|----------------------------------------------------------------------------------------------|-------------------------|------------------------|
| Phase 1: Requirements   | Privacy/data classification; compliance requirements; threat surface change identified       | On spec creation        | Every edit             |
| Phase 2: Design         | Threat model (STRIDE); trust boundaries; OpenAPI security rules; data flow review            | On design creation      | Per design             |
| Phase 3: Implementation | SAST (Semgrep + CodeQL); secrets detection (pre-commit); secure coding rules in linter       | Pre-commit + CI         | Every commit           |
| Phase 5: CI             | SAST; SCA (CVE + supply chain); IaC scan; container scan; licence compliance; SBOM           | Every PR                | Every PR               |
| Phase 6: Staging        | DAST (OWASP ZAP); security headers; TLS check; API security; pentest scripts                 | Every staging deploy    | Every deploy + nightly |
| Phase 7: Production     | Security header re-verification; WAF rule review; progressive rollout with anomaly detection | Every production deploy | Every deploy           |
| Phase 8: Operations     | Runtime security (Falco/eBPF); scheduled DAST; cloud posture (Prowler); certificate expiry   | Continuous + scheduled  | Continuous + weekly    |
| Policy as Code          | OPA / Conftest: no root containers, encrypted buckets and no wildcard IAM, enforced in CI    | Every PR + every deploy | Every PR + deploy      |

#### Observability as a design requirement

- Logging, tracing, and metrics requirements defined in Phase 1 (specification)

- SLI/SLO definitions written in Phase 2 (design), committed as configuration

- Observability implementation verified in Phase 3 (implementation), where CI checks that the required instrumentation is present

- Observability completeness verified post-deploy (signals actually flowing)

- SLO burn rate drives automated rollback in Phase 7 (production deployment)

#### Privacy and compliance

- PII classification triggered at spec creation; data flow documented in design

- Privacy impact assessment generated for specs touching personal data

- Compliance requirements (GDPR, PCI, SOC2, accessibility) checked at spec and design

- Runtime PII flow monitoring in production

- Data retention and deletion automation verified

#### Cost as a quality dimension

- Cost design: expected cloud cost per operation estimated in ADR

- Cost monitoring: per-service, per-feature in production

- Anomaly alerts when per-operation cost deviates significantly from estimate

- Cost trend feeds back to architecture reviews quarterly

### Phase 11: The meta-loop

Domain: Process. This phase periodically reviews the SDLC itself.

The SDLC is not a fixed system. LLM capabilities, tooling, and best practices evolve rapidly. The meta-loop exists to ensure the process itself stays current and effective.

| Dimension / Check     | What is Verified                                                                     | Trigger                   | Frequency     |
|---------------------------|------------------------------------------------------------------------------------------|-------------------------------|-------------------|
| Tool currency             | Are the SDLC tools still best-in-class? Have better alternatives emerged?                | Quarterly + on major releases | Quarterly         |
| LLM capability review     | Have model improvements unlocked automation that was previously aspirational?            | On major model releases       | Per major release |
| Gate calibration          | Are quality thresholds producing signal or noise? False positive rates tracked per gate. | Monthly                       | Monthly           |
| Escape analysis           | What slipped through despite all gates? Root cause and gate improvement.                 | After any escaped defect      | Per incident      |
| Automation coverage       | Are humans spending time on things that could now be automated?                          | Quarterly                     | Quarterly         |
| Process bottleneck        | Is the process itself introducing bottlenecks or friction?                               | Monthly cycle time review     | Monthly           |
| Prompt/skill optimisation | Are LLM prompts and skills still optimal for current model capabilities?                 | On major model releases       | Per major release |
| Security posture review   | Has the threat landscape changed? Are current controls still sufficient?                 | Quarterly                     | Quarterly         |
| Cost of the pipeline      | What is the CI/CD, LLM API, and tooling cost per deployment? Is it justified?            | Monthly                       | Monthly           |

One meta-loop calibration session can improve thousands of future automated runs, giving this human activity a long downstream effect.

## 3. Gaps in current automation

Some aspects remain difficult or impossible to automate fully. Track them as capabilities improve.

| Gap                     | Why It Is Hard                                                                                      | Current Best Approach                                                     |
|-----------------------------|---------------------------------------------------------------------------------------------------------|-------------------------------------------------------------------------------|
| Insecure design (OWASP A04) | No deterministic tool. Requires understanding intent and trust relationships.                           | Structured threat modelling step; LLM-assisted STRIDE review; human approval  |
| UX quality                  | Beyond accessibility: is it usable, delightful, appropriate? Requires human judgment.                   | Accessibility automated; usability requires user research and human taste     |
| Business logic correctness  | Does the code do what the business actually needs? Only testable against a complete spec.               | Spec quality gates (Phase 1: requirements and specification) are the primary lever here                       |
| Async / concurrency bugs    | Race conditions and deadlocks require formal verification or extensive fuzzing.                         | Partial: static analysis rules + concurrency-specific testing frameworks      |
| Emergent system behaviour   | How the system behaves under realistic load with real data is only partially simulatable.               | Chaos engineering + production monitoring together approximate this           |
| Full automated pentesting   | Meaningful pentest requires creativity and context that tools do not yet have.                          | Automated DAST + scheduled tools + periodic human-led pentest                 |
| Unknown unknowns in spec    | The spec cannot validate what it does not know it is missing. Discovery and iteration remain human-led. | Spec quality review catches known gap patterns; iteration is the safety valve |
| Strategic direction         | What to build, for whom, and why. Irreducibly human.                                                    | AI surfaces options and tradeoffs; human decides                              |

Phase 11 (the meta-loop) periodically re-evaluates these gaps as LLMs and deterministic tools improve.

## 4. Traceability

The full automation vision only becomes trustworthy when every artefact can be traced back to intent. Without traceability, automation produces outputs that cannot be verified against what was actually required.

The complete traceability chain:

| Artefact        | Links To                                              | Verified By                       |
|---------------------|-----------------------------------------------------------|---------------------------------------|
| Business goal / OKR | None                                                      | Product review                        |
| Feature spec        | Business goal / OKR                                       | Spec quality gate (Phase 1: specification)           |
| ADR                 | Feature spec; supersedes previous ADR if applicable       | Design review (Phase 2: design)               |
| OpenAPI spec        | Feature spec; ADR                                         | Spectral OWASP gate (Phase 5: CI pipeline)         |
| Threat model        | Feature spec; ADR; OpenAPI spec                           | Security design review (Phase 2: design)      |
| Code module         | ADR; feature spec (via acceptance criteria)               | SAST + design conformance (Phase 5: CI pipeline)   |
| Tests               | Acceptance criteria from spec (explicit reference)        | Mutation testing + coverage (Phase 5: CI pipeline) |
| PR                  | Feature spec; ADR; test results                           | PR traceability check (Phase 4: code review)       |
| Deployment          | PR; SBOM; test results; security scan results             | Deployment gate (Phase 7: production deployment)             |
| Incident postmortem | Affected spec; ADR; deployment that introduced regression | Feedback loop (Phase 9: feedback loops)               |

When automation disagrees with human expectation, validate the spec first. A valid spec should change the expectation; an invalid spec should be updated. Treating the spec as the source of truth makes this distinction explicit.

## 5. Implementation approach

### 5.1 Architecture of the automation system

- Deterministic tool layers such as linters, SAST, SCA and IaC scanners run as hard CI gates rather than LLM prompts

- LLM layers run on top of deterministic output: explaining findings, proposing fixes, generating drafts

- Policy as Code (OPA/Conftest) enforces security and compliance rules deterministically

- Confidence scoring aggregates signals from multiple gates into a single per-step signal

- Exception escalation routes low-confidence decisions to humans via structured notifications

- Feedback loops close automatically: findings create structured artefacts (specs, ADRs, regression tests)

### 5.2 Rollout sequencing

Not all phases are implemented simultaneously. Suggested sequencing by value and implementability:

| Wave | Focus                                                                             | Rationale                                                                   |
|----------|---------------------------------------------------------------------------------------|---------------------------------------------------------------------------------|
| 1        | Phase 5 (CI pipeline): SAST, SCA, secrets, linting, type-checking at maximum strictness    | Highest ROI, entirely deterministic, no LLM dependency, immediate quality floor |
| 2        | Phase 1 (specification): spec quality review. Phase 4 (code review): gates for size, description and traceability       | Addresses root cause (spec quality) and the PR bottleneck simultaneously        |
| 3        | Phase 6 (staging): DAST, load testing, security headers. Phase 7 (production deployment): auto-rollback          | Closes the production safety gap; enables confident deployment                  |
| 4        | Phase 9 (feedback loops): incident → spec, finding → regression test, usage → discovery | Compound improvement: the system gets better automatically over time            |
| 5        | Phase 11 (the meta-loop): gate calibration, tool review, LLM capability re-evaluation       | The process reviewing itself; only valuable once other phases are established   |

### 5.3 The spec-quality principle

The input spec bounds the trustworthiness of downstream automation. As automation increases, spec quality matters more. Invest disproportionately in Phase 1 (specification) gates and keep specs as the source of truth.
