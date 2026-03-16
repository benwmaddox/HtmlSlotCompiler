#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if [[ -f "$HOME/.cargo/env" ]]; then
  # Support local runs where cargo is installed via rustup but not on PATH yet.
  source "$HOME/.cargo/env"
fi

required_files=(
  "AGENTS.md"
  "CLAUDE.md"
  "Docs/AGENT_LOOP.md"
  "Docs/ARCHITECTURE.md"
  "Docs/BUGS.md"
  "Docs/CHANGELOG.md"
  "Docs/CHANGELOG_RULES.md"
  "Docs/NIGHT_SHIFT_REPORT.md"
  "Docs/QUALITY_GATES.md"
  "Docs/REVIEW_PERSONAS.md"
  "Docs/TESTING.md"
  "Docs/TODOS.md"
  "Scripts/nightshift.sh"
  "Scripts/validate.sh"
  "Scripts/verify-sample-build.sh"
  "rust/Cargo.toml"
)

for file in "${required_files[@]}"; do
  [[ -f "$file" ]] || { echo "Missing required file: $file"; exit 1; }
done

for file in Scripts/*.sh; do
  bash -n "$file"
done

allowed_states='READY|IN PROGRESS|DONE|NEEDS INPUT FROM USER'
for backlog in Docs/TODOS.md Docs/BUGS.md; do
  while IFS= read -r line; do
    state="${line#\#\# }"
    if [[ ! "$state" =~ ^($allowed_states)$ ]]; then
      echo "Unexpected state header in $backlog: $line"
      exit 1
    fi
  done < <(grep '^## ' "$backlog")
done

for path in \
  "Docs/AGENT_LOOP.md" \
  "Docs/REVIEW_PERSONAS.md" \
  "Docs/QUALITY_GATES.md" \
  "Docs/TESTING.md" \
  "Docs/ARCHITECTURE.md" \
  "Docs/TODOS.md" \
  "Docs/BUGS.md" \
  "Docs/CHANGELOG_RULES.md"; do
  grep -q "$path" AGENTS.md || { echo "AGENTS.md does not reference $path"; exit 1; }
done

grep -q '@AGENTS.md' CLAUDE.md || { echo "CLAUDE.md must import AGENTS.md"; exit 1; }
grep -q '@Docs/AGENT_LOOP.md' CLAUDE.md || { echo "CLAUDE.md must import Docs/AGENT_LOOP.md"; exit 1; }

cargo fmt --manifest-path rust/Cargo.toml --all --check
cargo check --manifest-path rust/Cargo.toml
cargo test --manifest-path rust/Cargo.toml
"$ROOT/Scripts/verify-sample-build.sh"

echo "Validation passed."
