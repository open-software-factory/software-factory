# `skills/`

This folder holds the skills this repository ships to adopters: [`show-me`](show-me/SKILL.md) and [`pr-outline`](pr-outline/SKILL.md).

## Installing a skill

Run `npx skills add open-software-factory/software-factory` and name the skill you want. Check `npx skills --help` for the exact command form, since it can change.

## Editing a skill

After you edit a skill, run `osf lint skill <folder>` on its folder, such as `osf lint skill skills/show-me`, and fix every error.

## Where other skills live

A skill that only an agent working on this repository itself uses lives in [`.agents/skills/`](../.agents/skills/) instead. A skill this repository ships and also uses itself keeps its one source here, in `skills/`.

Credit for a third-party skill, or any other third-party work this repository carries, lives in [`THIRD_PARTY_NOTICES.md`](../THIRD_PARTY_NOTICES.md).
