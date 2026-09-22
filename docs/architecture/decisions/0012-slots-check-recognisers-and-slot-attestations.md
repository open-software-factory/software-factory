# 0012: Slots, check recognisers and slot attestations

Status: accepted

Date: 2026-09-23

## Context

An adopting repository already runs a linter, a formatter and a test suite through its own tooling and wants those to count. The factory has to know when they do. A build that merely compiles fills no lint slot, and a test job that runs three tests fills no unit-test slot in any useful sense. The design is [the verification seam](../verification-seam.md). This record holds the decisions about how an adopter's own checks count, how the factory's defaults are stored, and how both reach a repository.

## Options considered

**How the factory judges "at least as strong".**

| Option | What it meant | Outcome |
|---|---|---|
| A check recogniser per ecosystem | The tool reads the adopter's workflow and project files against rules and reports each slot as filled or empty. For .NET the lint slot wants analysers at the latest level with warnings as errors and a style analyser package. | Taken, where a recogniser exists. |
| A slot attestation | The adopter writes in the configuration file that a slot is filled, with a reason and a date. | Taken, only for a slot no recogniser covers. Reported as the owner's word rather than a measured fact. |
| Always run the factory's own check and compare | Both run on every pull request and the findings are compared. | Set aside for pull requests. Taken as a periodic audit on the schedule. |

**The level of an empty slot the factory cannot fill itself.** Error blocks a new adopter on day one, because architecture tests and integration tests take time to write. Warning was taken. The adopter raises the slot to error in the configuration file when ready.

**The shape of the configuration file.**

| Option | What it meant | Why it was set aside |
|---|---|---|
| One table per slot | Everything about a slot under its name. | Taken. It answers the factory's question in one lookup and works with no CI at all. |
| One table per CI job | The file reads like the workflow and each job names the slots it fills. | The factory has to invert it, and levels and attestations need a second shape. |
| Almost no file | Everything inferred from workflow files, with attestations as comments in YAML. | Attestations lose their reason and date, and a repository with no CI has nowhere to put them. |

**Where the shipped defaults and the recogniser rules live.**

| Option | What it meant | Why it was set aside |
|---|---|---|
| One TOML data file per ecosystem inside the tool | Each entry holds the task shape, the slot, the checkpoints and the recogniser predicates from a small fixed set. | Taken. Adding a check is a data change and the catalogue is rendered from one source. |
| Rust code per ecosystem | A trait with a method for defaults and one for recognition. | Adding a check means a release, and the catalogue needs a generator that reads code. |
| Moon project fragments the tool copies in | The default is a real moon file the tool copies into the adopter's tree. | Two files per ecosystem drift apart, and the adopter's tree gains files to keep in sync. |

**How the checks reach an adopter's repository, and survive a factory release.**

| Option | What it meant | Why it was set aside |
|---|---|---|
| Generated files the factory owns | The tool renders the moon project, the workflows and the hooks from the defaults and the configuration file. Rendering is a pure function of version and configuration. A drift gate refuses hand edits and says where the edit belongs. | Taken. Updates are automatic and repeatable, and customisation has two named places. |
| Almost nothing in the repository | Only the configuration file and thin workflows that call the tool. | The adopter cannot read what runs against their code in their own tree. |
| Three-way merge on update | The adopter edits any file, and the tool merges on each release. | Conflicts need a person, and text merges of YAML break quietly. |

## Decision

A slot is a named kind of check the factory expects. A slot is filled by the factory's default, by the adopter's own task carrying the slot tag, by the adopter's CI job that a check recogniser confirms, or by a slot attestation. The aggregation reports how each slot was filled. A slot only the adopter can fill reports at warning until raised.

The configuration file `osf.toml` has one table per slot. An organisation-wide file with the same shape sits above it, and the repository file overrides one key at a time.

The defaults are one TOML file per ecosystem inside the tool. The order is Rust, .NET, Java, TypeScript, Python and Go. The first two ship together, and each of the rest enters when a real repository drives it. The catalogue page is rendered from these files.

The tool renders the files it owns into the adopter's repository. A daily task opens a pull request under the builder identity when a new factory release exists. The drift gate fails on a hand edit and names the key or the tag where the change belongs.

The two-word forms are the vocabulary. "Check recogniser" and "slot attestation", each with one plain sentence on first use in a document. Either word alone means too many things.

## Consequences

- The adopter's tree gains a small set of generated files and one configuration file, and nothing else.
- A periodic audit on the schedule compares slot attestations with completed runs, so an attestation does not stand on its own for long.
- A new predicate kind for the recogniser is added to the tool once, with its tests, and every ecosystem file may then use it.
- The stage template file proposed in [open-software-factory/software-factory#26 (verify stages as a template of slots)](https://github.com/open-software-factory/software-factory/issues/26) is replaced by the tags on moon tasks and the slot tables.
