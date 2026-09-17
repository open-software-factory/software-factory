# Classifying the risk of a change set

Research notes, compiled 2026-09-17 from six source surveys. They inform the design tracked in open-software-factory/software-factory#31 (risk classification). They are evidence for that design. They are not the design.

## 1. The question

A change set is any difference between two states of a codebase: two commits, a pull request, or two folders. Risk here means any issue the change set can cause once it is merged, built, or deployed. That covers a defect, an outage, a broken caller, or a leaked secret. It also covers a slower page, a larger bill, or a rule broken in a regulated domain.

The classifier's answer will decide three things. Whether a pull request may merge on its own. How many review axes run and how deep each one reads. Whether a person must sign off. A wrong low answer lets a dangerous change through. A wrong high answer spends review on a comment fix.

## 2. Three questions inside one word

Every source below separates the word risk into three questions that need different evidence.

| Question | Name used here | Evidence |
| --- | --- | --- |
| How far can this change reach? | reach, also called blast radius | files, symbols, modules, dependents, consumers, read from the diff and the dependency graph |
| What kind of thing does it touch? | hazard | categories such as identity, money, stored data, public contracts, user interface |
| How likely is it to be wrong? | likelihood | history of the touched code, size, author familiarity, tests present |

Reach and hazard are mostly facts a program can read. Likelihood is a prediction. Keeping the three apart keeps the report explainable.

## 3. What the research literature found

Just-in-time defect prediction is the field that scores a single commit for defect risk at the moment it is made. Each finding below names its source. A finding marked unverified is one the survey could not confirm.

**Stable features.** The features that predict a bad change have held for twenty-five years. Mockus and Weiss built the first change-level model in 2000, on a telecom switching system. It used files touched, subsystems touched, size, fix-or-feature, and author experience. Diffuse changes, those touching many files and subsystems, predicted failure best.

**Normalised size.** Raw lines changed barely predicts defects. Nagappan and Ball showed in 2005 that churn relative to file size, prior churn and time span predicted fault-prone binaries with 89 percent accuracy on a large operating system.

**Fourteen standard features.** Kamei and colleagues defined them in 2013, in five families, which are size, diffusion, purpose, history and experience. Their models on six open-source projects reached an area under the curve of 0.70 to 0.78. Under effort-aware evaluation, bugs found per line inspected, the models found about 35 percent of bugs while inspecting 20 percent of changed lines.

**A one-feature baseline held.** Zeng and colleagues showed in 2021 that a logistic regression on lines added alone matched or beat two deep-learning models. The test used a dataset eight times larger than the original, and the baseline trained tens of thousands of times faster. Pornprasit and Tantithamthavorn showed the same year that one deep model's reported gains came partly from a data leak. Fixing the leak cut its score by roughly 40 percent.

**Patterns drift.** McIntosh and Kamei tracked models over years of project history in 2018. The features that predicted risk shifted, and a model trained once degraded. Rolling retraining on recent data kept accuracy up.

**Labels are noisy.** Almost every dataset labels a bug-inducing commit by tracing a later fix back to the lines it changed. Rosa and colleagues in 2021, and Lyu and colleagues in 2023, found that method disagrees with developers often and silently misses commits. That noise caps how accurate any classifier can appear.

**Ownership predicts failure.** Bird and colleagues studied two operating system releases in 2011. The number of low-expertise contributors to a file and the top owner's share both predicted failures. Nagappan and colleagues had found in 2008 that organisational metrics out-predicted code metrics on the same product line.

**Context doubles the catch rate.** A 2025 industrial study, arXiv 2505.17928, built a review system that pulled in call-graph slices around a change. It caught twice the bugs of a plain diff-only model.

**Industrial deployments are thinly documented.** The survey found one described online-learning deployment on a live commit stream, and few named case studies of a risk model in daily use.

Five findings transfer to a small project. Change-level features, normalised size, a cheap baseline first, rolling retraining, and effort-aware evaluation. Three do not transfer. Models fitted on huge clean histories, gains reported on two or three large projects, and manual relabelling of noisy labels.

## 4. What large organisations do

The published systems split into two kinds. Review-time systems read the change before merge. Rollout-time systems watch the change after deploy. No published system gives one universal risk number.

| Organisation | Signal | What it gates | Reported outcome |
| --- | --- | --- | --- |
| Google, engineering book | change size and type | reviewer count and review depth; a 200-line target | small changes review faster and roll back safer |
| Google, reliability books | canary error rate and latency against a live control group | canary size, duration, go or no-go | roughly 70 percent of outages come from changes to a live system |
| Meta, test selection | history of test outcomes and flakiness | which tests run per change | about half the test cost, over 99.9 percent of breaking changes still caught |
| Meta, deployment system | health checks on system, call and business metrics; a dependency graph walk | automatic revert; cancels a release built on a bad shared library | about 14 percent of executables cancelled; 41 percent false positives on large services |
| Microsoft, rollout judge | fault logs and telemetry correlated to a rollout in time and place | automatic no-go | 92 percent precision, 100 percent recall on data-plane rollouts |
| Amazon, deployment guide | one-box stage, bake time, aggregate alarms | whether a pipeline advances | not stated |
| Slack | fleet metrics watched up to 10 percent rollout | automatic rollback within 10 minutes | 90 percent fewer customer-impact hours in a year |
| Netflix and Google, open canary judge | per-metric statistical test at 98 percent confidence | pass, marginal, or fail | a monitoring gap reads as a failure |

Google, Meta, Microsoft, Amazon, Slack and Netflix are the large software companies whose published systems fill the table. Patterns across them:

- Small changes are the common lever. Every system either limits change size or scores by it.
- The score gates a narrow decision each time: which tests, how deep a review, how big a canary. The decisions stay separate.
- Every system admits false positives and keeps a person or a slower check behind it.
- A safety gate is software too. One published outage came from a bug in the tool meant to stop the bad change.

## 5. Facts a program can read from a change set today

An inventory of deterministic tools, grouped by the fact they report. Every tool listed runs offline unless marked. Maturity was checked in September 2026.

**Structure and reach.** Tree differencing reports which syntax nodes changed instead of which lines: GumTree, a tree-differencing library, and difftastic, a structural diff over forty languages. Build graphs report which targets depend on a changed file. Bazel does it with reverse dependency queries. The monorepo tools Nx, moon, Turborepo and Pants do it with an affected-projects command. Test impact maps report which tests exercise a changed method.

**Public contracts.** One breaking-change detector exists per ecosystem, each comparing an interface before and after.

| Ecosystem | Tool | Reports |
| --- | --- | --- |
| Rust | cargo-semver-checks | one lint per semantic-versioning rule broken |
| Java | japicmp; revapi | binary and source compatibility between two archives |
| C# | the .NET SDK package validation | breaking changes against a baseline package |
| TypeScript | API Extractor | public surface changes against a committed snapshot |
| Go | apidiff | incompatible interface changes between two versions |
| Python | griffe | breaking changes in a package's public surface |
| Dart | dart_apitool | interface model diff with versioning verdicts |
| Kotlin | binary-compatibility-validator | a checked-in interface dump diff |
| wire formats | buf for protocol buffers; oasdiff for OpenAPI; GraphQL Inspector | breaking, dangerous and safe schema changes |
| cross-service | Pact | consumer expectations checked against a provider |

**Stored data.** Migration linters name the risky shape: squawk for PostgreSQL migrations, atlas for several databases. Both flag destructive operations and long locks as a category.

**Infrastructure.** A plan diff reports create, change and destroy counts: Terraform and its fork OpenTofu, Pulumi. Policy engines classify a plan against named rules: Open Policy Agent, Checkov. Live-cluster diffs need the cluster and do not run offline.

**What people see.** Pixel diffs against a baseline: Playwright snapshots and BackstopJS offline, two commercial services in the cloud. Accessibility rule violations with a rule id: axe-core. No dominant tool reports string or translation changes.

**Performance and volume.** Statistical benchmark regression: Bencher, criterion for Rust, pytest-benchmark for Python. Load thresholds: k6. Bundle size budgets for the web. No mature tool diffs a query plan.

**Sensitive content.** Secrets: gitleaks, trufflehog. Personal data entities with a confidence score: Presidio. Known vulnerable dependencies: osv-scanner, cargo-deny. The hosted CODEOWNERS feature reports ownership crossing.

**Size and churn.** Git itself gives lines added and deleted per file, rename detection with a similarity score, and binary markers. Hot spots and co-change coupling over history: code-maat.

**Gaps no offline tool fills.** Runtime behaviour change. How many real users a change affects. Whether a semantic change was intended. Reach across repositories without a shared build graph. Data correctness after a migration runs. Any one score across code, infrastructure and data. Whether a pixel change is visible to a person.

**Smallest useful set per ecosystem.** One breaking-change detector for the language, a secret scanner, and the affected-targets command of whatever build graph the repository already has.

## 6. Models as a second opinion

**Products.** Eleven review products were checked. Most emit findings with a severity. Two emit a priority or risk label. Two can run fully outside the vendor's cloud. None publishes a dataset, a ground truth, or a third-party replication for its accuracy claims. Treat every vendor accuracy figure as marketing until one does.

**Research.** A 2026 paper from Meta built a diff risk score from a model's attention over the diff, then mapped it to lines and hunks. Its top two flagged hunks held 54 percent of the risky lines while covering 26 percent of the changed code. [reported: arXiv 2607.02782, unverified by this author] A 2023 paper, arXiv 2308.11148, showed small fine-tuned models matching dedicated review models at under 7 billion parameters.

**Small local classifiers.** Few-shot sentence-embedding classifiers of 110 to 355 million parameters matched a large model trained on 3,000 examples with 8 examples per class. [reported: SetFit] Fine-tuned code encoders reach a macro F1 near 0.74 on binary change classification. [reported: arXiv 2605.01596] A retrieval approach that labels a commit by similarity to past commits ran up to 112 times faster than the learned models. [reported: arXiv 2210.02435] A pure-Rust inference path exists. A 150-million-parameter zero-shot classifier loaded through the candle crate and cached offline is enough for a first local layer.

**Inputs that help.** Call-graph slices doubled the catch rate. Retrieval of related code raised bug-detection accuracy by 31 percent in one study. Ownership is a validated signal. The diff alone is not worthless: attention over it already localised risk.

**Reproducibility has a limit.** Temperature zero reduces but does not remove randomness. Batch size, kernel choice and floating-point ordering differ across hardware, so the same weights can answer differently on two machines. [reported: arXiv 2308.02828; 2604.27006; 2604.22411] A pinned weight hash controls one variable. The report must record the hash, the hardware class and the runtime version, and a golden set must run on the reference machine.

**Calibration.** Two public change-level datasets exist, one of 106,674 labelled commits and one of 213,000 files. A repository can label its own history: a change is risky if it was later reverted, hot-fixed, or linked to an incident. One study reached 86 percent accuracy from one project's history alone. [reported: arXiv 2411.05230] No source states a precision bar for auto-merge. That number is a local policy. No source covered drift monitoring, so it must be built by re-running the labelled set on a schedule.

**A floor the model cannot lower.** No published policy was found where rules set a minimum tier and a model may only raise it. It is a sound pattern. It is not yet a proven practice.

## 7. Hazard categories

The regulated domains each ask a fixed set of questions about a change. Safety standards ask whether a change touches a safety requirement and how much must be re-verified. Financial reporting controls ask who wrote and who approved a change to ledger code. Payment card rules require change control on anything in the cardholder environment. Privacy law requires an impact assessment when a change adds sensitive data, tracking, or automated decisions. Security guidance asks for review of any change to authentication, authorisation, cryptography, input handling, or secrets. Platform providers define a breaking change as a removed or renamed field, a changed type, a new required parameter, or a changed default.

Those questions, joined with the signals the deterministic floor already reads, give the categories below. Each row names the deterministic signal a diff carries and the context a graph or a model must add. Each row also says whether a named standard demands a person.

| Category | Touches | Deterministic signal in the diff | Context needed | Human sign-off in a named standard |
| --- | --- | --- | --- | --- |
| identity and access | login, sessions, roles, permissions, secrets | paths and symbols in auth modules; secret scanner hits | whether the path is reachable from an untrusted input | yes, security guidance |
| money | ledger, billing, pricing, valuation, payment capture | paths and symbols in those modules; currency and amount types | which flows read the changed value | yes, financial and payment rules |
| personal and health data | fields, logging, sharing, retention | new columns or fields named like personal data; new outbound calls; personal-data detector hits | data category of the store; whether it leaves the system | yes, privacy law |
| stored data shape | migrations, schemas, serialised types, wire formats | migration linter findings; schema diff; serialisation attribute changes | live volume of the table; consumers of the format | yes, operations review |
| public contract | exported symbols, routes, flags, events, file formats | breaking-change detector findings per ecosystem | who consumes it: people, agents, other programs; deprecation policy | yes for a breaking change, versioning policy |
| external interface | third-party calls, webhooks, queues, protocols | new hosts, changed request shapes, changed retry or timeout values | the partner's contract and rate limits | no fixed standard |
| user interface | screens, copy, defaults, prices, terms, focus order | interface file changes; pixel diff; accessibility rule hits; string changes | whether the change is visible and to whom | yes for accessibility in public sector; consumer rules on terms |
| high-traffic paths | hot code, shared libraries, request paths | dependency-graph fan-in; call counts where telemetry exists | traffic share of the path | no fixed standard |
| performance and volume | loops, queries, batch sizes, caches, indexes | benchmark regression; query change; collection iteration inside a loop | expected data volume | budgets, internal policy |
| infrastructure and delivery | workflows, containers, plans, feature flags | plan destroy counts; policy engine findings; workflow file changes | which environment | yes, operations review |
| supply chain | dependencies, lockfiles, build provenance | new dependency; major bump; licence change; install scripts | reputation of the package | yes at higher provenance levels |
| cost | cloud resources, retention, egress | plan resource counts; storage class or retention changes | unit prices | internal policy |
| tests and verification | test files, skips, fixtures | test deleted or skipped; source changed with no test change | which behaviours the deleted test covered | no fixed standard |

## 8. What this changes in the design

The design in open-software-factory/software-factory#31 (risk classification) has three layers: a deterministic floor, a local model, and a combination rule. The research supports the shape and changes the content.

- **Report reach as its own numbers.** Files, symbols, modules, dependents from the build graph, and consumers from the contract detectors. Reach is a fact and belongs in every report, whatever the tier.
- **Report hazard as categories.** Take them from the table above. Each category is a small reader per ecosystem, and each carries the sign-off flag from the standards.
- **Add the validated likelihood features.** Lines added, churn relative to file size, files and subsystems touched, prior fixes to the same lines, author familiarity, ownership share, tests present. Start with lines added as the baseline the model must beat.
- **Give the model context.** Treat it as a second opinion. A call-graph slice around the change, the repository map, and the reach and hazard facts. The model returns a tier, a confidence, a rationale, and the axes it thinks are needed. The floor cannot be lowered.
- **Record what produced the answer.** Signal versions, model weight hash, hardware class, runtime version. Without those, two reports cannot be compared.
- **Measure with an effort-aware metric.** Bugs found per line the reviewer reads, on a labelled set built from this repository's own reverts, hot-fixes and incidents. Retrain on a rolling window. Re-run the set on a schedule to catch drift.
- **Keep the gate narrow.** The tier decides review axes and depth, rounds, and human sign-off. Rollout decisions stay with the deployment system, which has its own signals and its own false-positive rate.

## 9. Open questions

- How to size reach for a change with no build graph in the repository, beyond file and directory counts.
- Which small model to start with for the local layer, and on which reference machine the golden set runs.
- What the auto-merge precision bar should be. No published number exists; it is a policy the owner sets.
- Whether a query-plan diff and a string-change detector are worth building, since no offline tool provides either.
- How to label this repository's own history when it has no incident tracker yet.

## 10. Sources

Papers, by the label used above.

- Mockus and Weiss 2000, "Predicting risk of software changes", Bell Labs Technical Journal 5(2).
- Nagappan and Ball 2005, "Use of relative code churn measures to predict system defect density", ICSE.
- Nagappan, Murphy and Basili 2008, "The influence of organizational structure on software quality", ICSE.
- Bird et al. 2011, "Don't touch my code! Examining the effects of ownership on software quality", FSE.
- Kamei et al. 2013, "A large-scale empirical study of just-in-time quality assurance", IEEE Transactions on Software Engineering 39(6).
- McIntosh and Kamei 2018, "Are fix-inducing changes a moving target?", IEEE Transactions on Software Engineering 44(5).
- Hoang et al. 2019, "DeepJIT", MSR; Hoang et al. 2020, "CC2Vec", ICSE.
- Zeng et al. 2021, "Deep just-in-time defect prediction: how far are we?", ISSTA.
- Pornprasit and Tantithamthavorn 2021, "JITLine", MSR.
- Rosa et al. 2021, "Evaluating SZZ implementations through a developer-informed oracle", ICSE; Lyu et al. 2023, arXiv 2308.05060.
- Machalica et al. 2019, "Predictive test selection", ICSE SEIP.
- Li et al. 2020, "Gandalf: an intelligent, end-to-end analytics service for safe deployment in large-scale cloud infrastructure", NSDI.
- Grubic et al. 2023, "Conveyor: one-tool-fits-all continuous software deployment at Meta", OSDI.
- Keshavarz and Nagappan 2022, "ApacheJIT", arXiv 2203.00101; Mahbub et al. 2023, "Defectors", arXiv 2303.04738.
- arXiv 2210.02435, 2308.02828, 2308.11148, 2411.05230, 2505.17928, 2604.22411, 2604.27006, 2605.01596, 2607.02782: identifiers as reported by the survey; the 2026 entries were not opened by this author.

Books and guides.

- "Software Engineering at Google", the code review chapter; "Site Reliability Engineering" and its workbook, the release engineering, canarying and introduction chapters.
- The Amazon Builders' Library, "Automating safe, hands-off deployments" and "Ensuring rollback safety during deployments".
- Slack Engineering, "Deploy safety: reducing customer impact from change", 2025.
- The Spinnaker canary judge documentation, for the Kayenta method.
- The DORA 2024 Accelerate State of DevOps report, for change failure rate and rework rate.
- The AWS Well-Architected Framework reliability pillar, for staggered deployment guidance.

Standards and rules.

- Safety: ISO 26262, DO-178C, IEC 62304, and the FDA guidance on software changes to a marketed device.
- Finance and payment: SOX section 404 with COBIT, and PCI DSS requirement 6.
- Privacy and health: GDPR article 35, and the HIPAA security rule.
- Security: NIST SP 800-53 control CM-3, the NIST Secure Software Development Framework, SLSA v1.0, OWASP Top 10 2025 and the authentication cheat sheet.
- Interfaces: WCAG 2 level AA, semantic versioning 2.0, and the public API versioning policies of three platform providers.

Tools are linked from their names in the tool inventories of the underlying surveys; each name above is enough to find the project.
