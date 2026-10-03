# Rules for `crates/`

## Code layout

- Keep the `osf` binary a thin command-line shell. A command there parses its arguments, calls a library crate, and prints the result. It holds no logic of its own.
- Put the logic in library crates under `crates/`, one crate per logical group, the way `osf-lint-core` holds the lint rules. One crate per command is too fine.
- Give a functional group its own crate. The review check goes in `osf-review`. Pull request work, such as the status block and description sections, goes in `osf-pr`. The checkpoint runner goes in `osf-checkpoint`.
- Give a provider its own crate, named after the provider and matching its command group. For example, `osf github …` lives in `osf-github`. Give another code host, tracker or coding-agent harness a crate of its own when you add it.
- Keep a functional crate free of any one provider. Reach a provider through an interface.
- Put a new tool into the crate for its group. Start a new crate only when no group fits.

## Language

- Prefer strict, statically typed, fast languages: Rust, C#, or TypeScript.
- Use Go only when a task has to be in Go.
- Keep the engine, its command-line interface, and the verifier runner in Rust.
- You may write a provider at the factory's edges in any of these languages and reach it out of process.

## Tests and checks

Before you push a change under `crates/`, run these from the repository root, and make all four pass:

```
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
```

A warning from `cargo clippy` fails the build on purpose. `.github/workflows/ci.yml` is the one source of truth for what a pull request must pass. Read it to reproduce any step by hand.
