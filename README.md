# Software Factory

Software Factory is an agent-native operating environment for shipping high-quality software. It is meant to coordinate work across existing coding agents, work-item systems, repositories, deterministic verification tools, runners and delivery systems. Visibility, traceability and operator control are kept throughout.

It is for engineers who run coding agents over their own repositories and want that work verified, recorded and visible.

The project is moving from vision and research toward its first architecture and implementation. There is no runnable factory yet. What runs today is `osf`, a command-line tool: it lints prose and skill files, scans for text that must never reach a public repository, reports the risk of a change, and keeps a status block on a pull request. Build it and try it on this repository:

```
cargo run -p osf -- --help
cargo run -p osf -- lint writing README.md
```

## Current direction

- The Factory Engine starts in Rust: small, native, strongly typed and cross-platform.
- Coding agents, work trackers, forges, runners and other providers remain replaceable integrations.
- Existing skills and deterministic ecosystem tooling should be composed rather than cloned.
- The operator experience is an active control surface, not a read-only dashboard or a chat wrapper.
- Intent, work, implementation, verification, delivery and production evidence should remain traceable.

These are implementation constraints, not a claim that the wider ecosystem must use Rust.

## Document map

- [`docs/vision/`](docs/vision/): the durable product, lifecycle and operating-model vision.
- [`docs/product/ux/`](docs/product/ux/): the current operator experience and design principles.
- [`docs/product/decisions/`](docs/product/decisions/): accepted product decisions.
- [`docs/architecture/decisions/`](docs/architecture/decisions/): accepted and provisional technical decisions.
- [`docs/architecture/open-questions.md`](docs/architecture/open-questions.md): questions to resolve through research and implementation.
- [`docs/research/`](docs/research/): supporting investigations and changing ecosystem evidence. They are not requirements.
- The work itself lives in the organisation's GitHub project, as issues with native types, parents and fields.

Repository-specific guidance for coding agents is in [`AGENTS.md`](AGENTS.md).

## Development container

[`docs/development.md`](docs/development.md) covers the development
container. It states how to open it, what its git hooks check, what they
cannot do, and how to run the same checks by hand.

## Licence

This project is offered under Apache-2.0 or MIT, at your option, in
[`LICENSE-APACHE`](LICENSE-APACHE) and [`LICENSE-MIT`](LICENSE-MIT), and
[`CONTRIBUTING.md`](CONTRIBUTING.md) says how to send a change.
