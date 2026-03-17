# AGENT_LOOP.md

## Prime directive

Run autonomously without requiring plan review. Own validation and leave the repository in a reviewable state.

## Preparation

1. Inspect `git status --short`.
2. Inspect `git branch --show-current` and preserve the current branch when the run was launched to revise an existing PR. For issue-driven or spec-driven work, start from the repo default branch after it has been synced with `origin`, then create a fresh `nightshift/...` branch.
3. If the tree is dirty, either create a protective WIP commit or stop and explain why the state is unsafe to modify.
4. Run the quality gates in `Docs/QUALITY_GATES.md`.
5. If validation fails, fix it first or move the task to `NEEDS INPUT FROM USER` with evidence.

## Choose work

1. Read `Docs/BUGS.md`; choose the highest-severity item in `READY`, including any PR review feedback synced there by the inbox.
2. If no bug is ready, choose the oldest non-draft spec in `Specs/`.
3. If no implementation task is available, improve docs, validation, or task hygiene.

## Understand the task

- Read the chosen spec or bug entry.
- If the bug came from synced PR review feedback, use the included GitHub links and thread context to understand exactly what needs a reply.
- Load only the docs needed for that task.
- Read the relevant Rust code before proposing changes.

## Tests-first workflow

1. Write a brief testing plan in working notes or commit history, not for human review.
2. Add or expand automated checks to capture the desired behavior.
3. Run the checks and confirm they fail for the expected reason before implementation.

## Reviewer gate before implementation

- Run the personas in `Docs/REVIEW_PERSONAS.md`.
- If any persona is `BLOCKED`, update docs, tests, or plan before changing code.

## Implement

- Make the smallest change that satisfies the failing checks.
- Run the full quality gates after each meaningful change.

## Reviewer gate after implementation

- Re-run the personas against the diff.
- Iterate until all personas are `GREEN` or the task is explicitly blocked by missing user input.

## Wrap-up

1. Add or update a changelog entry following `Docs/CHANGELOG_RULES.md`.
2. Update any docs that would prevent repeating the same mistake.
3. If the task came from PR review feedback, reply on GitHub when appropriate with the fix, clarification, or follow-up question.
4. Commit with a message that explains what changed, why, how it was verified, and any residual risks.
5. Append a concise entry to `Docs/NIGHT_SHIFT_REPORT.md`.

## Stop conditions

- No `READY` bugs remain and no runnable specs remain.
- The task requires a product, design, or business decision from the user.
- Validation cannot be restored safely within the current run.
