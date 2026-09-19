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

The checks a pull request must pass are defined in
`.github/workflows/ci.yml`. That file is the single source of truth: read it
to see what runs, and to work out how to reproduce any step on your own
machine before you push.

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

## Code of conduct

This project has a [code of conduct](CODE_OF_CONDUCT.md). It applies to every
space the project runs, including issues, pull requests, and reviews.

## The contributor agreement

You must sign a contributor licence agreement before we can accept your change.
A contributor licence agreement is a short document that grants this project the
right to use what you wrote.

The text of the agreement is in [`CLA.md`](CLA.md). It is adapted from the
template published by the Apache Software Foundation, the non-profit body
behind many widely used open-source projects.

Signing grants this project a licence to your contribution. That keeps the
whole codebase under one owner, so the licence can change later if the project
needs it to. You keep your own copyright.

If this is a problem for you, open an issue and say so before you write code.

### How to accept it

Do one of these two. The first is enough for most contributors.

1. On your first pull request, post this comment, with nothing changed:

   > I have read CLA.md and I agree to it.

   Your GitHub account name and the time of the comment are the record that
   you accepted. A maintainer checks for it before merging.

2. If you would rather sign a paper copy, fill in the form in
   [`CLA.md`](CLA.md), sign it, and send it to the contact address published
   on this project's organisation page on GitHub. Then say in your pull
   request that you have sent it.

Accept once. It covers every change you send afterwards.

If you write code as part of a job, your employer may own what you write.
Check before you accept, because clause 4 of the agreement asks you to state
that you have the right to grant the licence.

## Licence

The Apache License 2.0 is a permissive open-source licence that also grants
patent rights. The MIT licence is a shorter permissive licence with no patent
terms. This project is offered under both, and you may use either one. See
`LICENSE-APACHE` and `LICENSE-MIT`.

Any contribution you submit is offered under the same two licences.
