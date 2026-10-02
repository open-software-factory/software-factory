# Execution platform trust properties

Status: draft. This document states principles for later enforcement. It becomes a decision record only after a design session.

Date: 2026-09-30

Decision 0020 is called "Who can post a review result the factory trusts". It is [open-software-factory/software-factory#136 (the review check design)](https://github.com/open-software-factory/software-factory/issues/136). It settles the review gate on one platform, GitHub Actions. This document names the general properties behind that record. Any gate, on any platform, needs these properties to be safe.

## Problem

A gate checks a change. A gate blocks or allows a merge. The review check is one gate. A gate is only as safe as the platform that runs it.

The factory plans to support more than one execution platform over time. GitHub Actions is the first platform, and other platforms come later.

The factory holds no keys. The factory runs no servers. The factory builds no firewalls. Each platform supplies its own keys, its own servers, and its own network controls. The factory states what it needs, and it reads what the platform actually gives.

## Principle: declare, map, check, degrade

The factory follows one loop for every gate, on every platform.

- **Declare.** The factory states which trust properties a gate needs.
- **Map.** Each platform adapter turns that need into the platform's own configuration.
- **Check.** The factory reads the platform's settings through the adapter. It reports each property as met or not met.
- **Degrade.** A gate on a platform that does not meet a needed property reports could-not-run, or it runs as advisory only. The report names the missing property. The gate never claims a pass it cannot back.

This loop carries forward the rule in [decision 0003](decisions/0003-deterministic-verification-is-authoritative.md). A read separates could-not-read from nothing there, and green is earned. Here, the read checks the platform's own settings instead of a check's result.

## The properties

| Property | Meaning | GitHub Actions mechanism | Minimum or Recommended |
| --- | --- | --- | --- |
| Trusted definition | The job that judges a change comes from the protected branch. | `pull_request_target` | Minimum |
| Gate | The job's outcome blocks the merge. | A required check in branch protection. | Minimum |
| Scoped secrets | Only a trusted-definition job can use the keys. | An environment, limited to the main branch. | Minimum for the environment. Recommended for the branch limit. |
| Isolation | Each job runs in a fresh environment. The change is mounted read-only. | A hosted runner, with `docker run` and a read-only mount. | Minimum (built in) |
| Egress control | Only listed hosts are reachable from the job. | harden-runner, in block mode. | Minimum (built in) |
| Outside approval | A person approves a run that comes from outside the project, before it can use trusted keys. | Off by default. `OSF_REVIEW_FORKS`, plus a protected fork-review environment, turn it on. | Recommended |
| Short-lived identity | The verifier posts its result with a token that expires soon. | An app token, minted fresh for each job. | Minimum |
| Key never exposed | The job can use a key. The job never reads the key's value. | Today, this is not met on GitHub-hosted runners. A later option is a self-hosted, ephemeral runner with a proxy on the host. The proxy adds the key to model API requests. The job itself never sees the key. | Future |
| Least privilege for the builder | The builder app cannot change a setting a gate later reads. | Remove `actions_variables: write` from the builder app. | Recommended |

## Adopter minimum today, for the review gate

An adopter who wants the review gate needs three things today:

- the verifier app's ID and its key
- one environment, named `review`, holding two model keys
- branch protection that requires the review job

## Future work

None of this exists yet. Three pieces of work follow from this document.

1. A posture check. It reports each property, for each repository. It sits next to [open-software-factory/software-factory#153 (the CI authority and weakening check)](https://github.com/open-software-factory/software-factory/issues/153).
2. An `osf github setup` command. It creates the environments, with the branch limit. It sets branch protection. It prompts for the keys. It runs the posture check.
3. Adapters for other execution platforms.

## Status of this document

This document is a draft. It states principles. It does not give commands. It does not enforce anything yet. A design session must review it before it becomes a decision record.
