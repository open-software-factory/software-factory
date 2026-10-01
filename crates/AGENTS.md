# Rules for `crates/`

## Code layout

- The `osf` binary is a thin command-line shell. A command there parses its arguments, calls a library crate, and prints the result. It holds no logic of its own.
- The logic lives in library crates under `crates/`, one crate per logical group, the way `osf-lint-core` holds the lint rules. One crate per command is too fine.
- A functional group gets its own crate. The review check goes in `osf-review`. Pull request work, such as the status block and description sections, goes in `osf-pr`. The checkpoint runner goes in `osf-checkpoint`.
- A provider gets its own crate, named after the provider and matching its command group. For example, `osf github …` lives in `osf-github`. Another code host, tracker or coding-agent harness gets a crate of its own when it is added.
- A functional crate stays free of any one provider. It reaches a provider through an interface.
- A new tool goes into the crate for its group. Start a new crate only when no group fits.
