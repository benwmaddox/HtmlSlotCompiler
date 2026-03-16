# ARCHITECTURE.md

## Layout

- `rust/` contains the `site-compiler` Rust crate and unit tests.
- `sample/src/` contains a smoke-testable sample site with layout, pages, includes, and assets.
- `.github/workflows/nightly-release.yml` publishes nightly release archives from `master`.
- `dist/` is reserved for lightweight distribution docs; release binaries should come from CI artifacts and GitHub releases, not from tracked executables in the repository.

## Validation strategy

- Run Rust formatting, static checks, and tests against `rust/Cargo.toml`.
- Run a sample build smoke test so validation covers real HTML compilation behavior, not only unit tests.
- Keep validation entrypoints deterministic and non-interactive so they can run in CI and overnight loops.

## Expected extension points

- Add task specs under `Specs/`.
- Grow the Rust test suite when compiler behavior changes.
- Add more fixture-based smoke tests if slot modes, include semantics, or asset copying become more complex.
