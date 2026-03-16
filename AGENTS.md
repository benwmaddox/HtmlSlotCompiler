# AGENTS.md

## Purpose

This repository uses a Night Shift style workflow for autonomous work on the Rust CLI and its sample site fixtures.

## Non-negotiables

- Prefer deterministic validation over opinions.
- Fix docs, workflow, and tests when the failure comes from missing guidance.
- Keep changes small, reviewable, and easy to audit.
- Work on one task at a time.
- Leave a concise morning report for the human reviewer.

## Instruction routing

- Loop contract: `Docs/AGENT_LOOP.md`
- Review personas: `Docs/REVIEW_PERSONAS.md`
- Quality gates: `Docs/QUALITY_GATES.md`
- Testing commands: `Docs/TESTING.md`
- Architecture notes: `Docs/ARCHITECTURE.md`
- Backlog: `Docs/TODOS.md`
- Bug queue: `Docs/BUGS.md`
- Changelog policy: `Docs/CHANGELOG_RULES.md`

## Repo-specific guidance

- The production crate lives under `rust/`.
- Validation must target `rust/Cargo.toml`, not the repo root.
- Use `sample/src/` for smoke-test verification of real compiler behavior.
- Do not modify or replace `dist/HtmlSlotCompiler.exe` unless the task explicitly requires a release artifact refresh.

## Task selection

- Pick the highest-severity `READY` bug from `Docs/BUGS.md` first.
- If no `READY` bug exists, pick the oldest non-draft spec in `Specs/`.
- If neither exists, improve docs, tests, or validation and record that work.

## Output contract

- Do not ask the human to review plans during the run.
- Leave evidence in tests, docs updates, commit messages, and `Docs/NIGHT_SHIFT_REPORT.md`.
