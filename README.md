# Software Factory

Software Factory is an agent-native operating environment for shipping high-quality software. It is intended to coordinate work across existing coding agents, work-item systems, repositories, deterministic verification tools, runners and delivery systems while preserving visibility, traceability and operator control.

The project is moving from vision and research toward its first architecture and implementation. There is no runnable factory yet.

## Current direction

- The Factory Engine starts in Go: small, native, strongly typed and cross-platform.
- Coding agents, work trackers, forges, runners and other providers remain replaceable integrations.
- Existing skills and deterministic ecosystem tooling should be composed, not cloned.
- The operator experience is an active control surface, not a read-only dashboard or a chat wrapper.
- Intent, work, implementation, verification, delivery and production evidence should remain traceable.

These are implementation constraints, not a claim that the wider ecosystem must use Go.

## Document map

- [`docs/vision/`](docs/vision/): the durable product, lifecycle and operating-model vision.
- [`docs/product/ux/`](docs/product/ux/): the current operator experience and design principles.
- [`docs/product/decisions/`](docs/product/decisions/): accepted product decisions.
- [`docs/architecture/decisions/`](docs/architecture/decisions/): accepted and provisional technical decisions.
- [`docs/architecture/open-questions.md`](docs/architecture/open-questions.md): questions to resolve through research and implementation.
- [`docs/research/`](docs/research/): supporting investigations and changing ecosystem evidence; these are not requirements.
- [`todo.md`](todo.md): disposable bootstrap work, not the eventual factory backlog model.

Repository-specific guidance for coding agents is in [`AGENTS.md`](AGENTS.md).

## Licence

Apache-2.0 or MIT, at your option: see [`LICENSE-APACHE`](LICENSE-APACHE) and
[`LICENSE-MIT`](LICENSE-MIT). To contribute, see [`CONTRIBUTING.md`](CONTRIBUTING.md).