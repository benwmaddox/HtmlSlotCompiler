# QUALITY_GATES.md

Run these gates for every Night Shift task:

1. `./Scripts/validate.sh`
2. Any task-specific verification required by the chosen bug or spec

Rules:

- Do not finish a task with failing gates.
- If a gate is flaky, document that explicitly and either stabilize it or move the task to `NEEDS INPUT FROM USER`.
- Keep the validation entrypoint deterministic and non-interactive.
