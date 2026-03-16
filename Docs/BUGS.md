# BUGS.md

Allowed states: `READY`, `IN PROGRESS`, `DONE`, `NEEDS INPUT FROM USER`

## READY

<!-- NED-INBOX:START -->
- [P1][PR #4] Address review feedback for `Add Night Shift workflow and validation`
  - Source: https://github.com/benwmaddox/HtmlSlotCompiler/pull/4
  - Synced at: `2026-03-16T18:15:38.383758+00:00`
  - Review decision: `CHANGES_REQUESTED`
  - Review by benwmaddox: See comment
  - `Docs/ARCHITECTURE.md:8` Now that we have a release process, I think we shouldn't have any executables included in the git repo directly.
  - `Scripts/nightshift.sh:46` I think we need --yolo for this. And if we don't define $MODE, I want it to default to codex.
<!-- NED-INBOX:END -->

## IN PROGRESS

- None.

## DONE

- None.

## NEEDS INPUT FROM USER

- None.
