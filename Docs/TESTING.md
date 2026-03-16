# TESTING.md

## Standard validation

Run the shared validation entrypoint:

```bash
./Scripts/validate.sh
```

## Under the hood

`./Scripts/validate.sh` runs:

```bash
cargo fmt --manifest-path rust/Cargo.toml --all --check
cargo check --manifest-path rust/Cargo.toml
cargo test --manifest-path rust/Cargo.toml
./Scripts/verify-sample-build.sh
```

## Notes

- Source `"$HOME/.cargo/env"` first if `cargo` is not already on `PATH`.
- The sample build smoke test verifies slot merging, include expansion, and asset copying against `sample/src/`.
