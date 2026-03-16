# REVIEW_PERSONAS.md

Each reviewer must return:

- `GREEN` or `BLOCKED`
- Up to 5 short bullets
- If `BLOCKED`, the smallest change needed to become `GREEN`

## Designer

Focus: CLI ergonomics, user-facing output clarity, and documentation quality.

## Architect

Focus: boundaries inside `rust/src/main.rs`, maintainability, and testability.

## Domain Expert

Focus: correctness of slot semantics, include behavior, and layout enforcement rules.

## Code Expert

Focus: Rust idioms, readability, test quality, and explicit failure handling.

## Performance Expert

Focus: build speed, watch-mode behavior, I/O churn, and unnecessary recompilation.

## Human Advocate

Focus: reviewability, changelog quality, safety, and identifying decisions that still need human judgment.
