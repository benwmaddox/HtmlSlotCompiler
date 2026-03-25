# CHANGELOG.md

## 2026-03-25

- Issue #6: added nested page compilation with nearest-ancestor `_layout.html` resolution, nested output paths, and matching sample coverage.
  - Verification: `./Scripts/validate.sh`
  - Risk: watch mode still rebuilds all pages when any layout or component HTML changes, which is correct but not yet optimized.

## 2026-03-16

- Added Night Shift workflow docs, validation scripts, and CI checks for the Rust crate and sample build.
  - Verification: `./Scripts/validate.sh`
  - Risk: overnight work quality still depends on keeping `Docs/BUGS.md`, `Docs/TODOS.md`, and `Specs/` current.
