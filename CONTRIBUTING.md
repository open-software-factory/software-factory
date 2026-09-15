# Contributing

Thank you for looking at this project. This page says how to build it, what the
checks expect, and what you must agree to before a change can be accepted.

## Build and test

The engine is a Rust workspace under `crates/`.

```
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets
cargo fmt --all --check
```

All four must pass. The workspace treats Rust linter warnings as errors, so a
warning fails the build on purpose.

## Run the checks before you push

The project checks itself with its own tool.

```
cargo run --bin osf -- verify --stage pre-push
```

This runs the same checks that run on a pull request. If it passes here, it
passes there. If the two ever disagree, that is a bug worth reporting.

## What the checks look for

The tool checks prose as well as code. It is stricter than most projects,
because the documents here are read by software agents as well as by people.
A vague sentence causes a wrong action. The cost goes well past a confused reader.

Run `osf explain <rule-id>` for any finding. Every rule states what it checks,
why it exists, and whether it comes from an external standard, a published
measurement, or this project's own taste. A rule that is only taste says so.

If a finding is wrong, you can silence it in place with a comment, and you must
give a reason:

```
<!-- osf-disable-next-line long-sentence -- quoting a specification verbatim -->
```

A suppression with no reason is itself an error. Suppressions are reported and
counted, and the checks that run on a pull request ignore them, so nothing is
hidden from review.

## Commit messages

Write the subject in the present tense, and do not end it with a full stop.
Explain in the body what changed and why. Keep sentences short.

A message must stand on its own. Do not refer to a conversation, a chat, a
plan, or a numbered item that a reader of this repository cannot see.

Use a reference that a reader can resolve. Write `open-software-factory/software-factory#12`
with a short label saying what it is. A bare number leaves the reader guessing.

## The contributor agreement

You must sign a contributor licence agreement before we can accept your change.
A contributor licence agreement is a short document that grants this project the
right to use what you wrote. Ours follows the template published by the Apache
Software Foundation, which is the non-profit body behind many widely used
open-source projects.

The reason is plain, and we would rather state it than hide it. Signing gives
this project the rights to your contribution. That keeps the whole codebase
under one owner, which means the licence can be changed later if the project
ever needs to. Without it, every past contribution would have to be removed or
rewritten first.

You keep your own copyright. You are granting a licence, not giving your work
away.

If this is a problem for you, open an issue and say so before you write code.
We would rather talk about it early than turn down work you have already done.

## Licence

The Apache License 2.0 is a permissive open-source licence that also grants
patent rights. The MIT licence is a shorter permissive licence with no patent
terms. This project is offered under both, and you may use either one. See
`LICENSE-APACHE` and `LICENSE-MIT`.

Any contribution you submit is offered under the same two licences.
