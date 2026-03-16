#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if [[ -f "$HOME/.cargo/env" ]]; then
  source "$HOME/.cargo/env"
fi

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

OUT_DIR="$TMP_DIR/dist"

cargo run --quiet --manifest-path rust/Cargo.toml -- sample/src "$OUT_DIR"

[[ -f "$OUT_DIR/index.html" ]] || { echo "Missing built index.html"; exit 1; }
[[ -f "$OUT_DIR/about.html" ]] || { echo "Missing built about.html"; exit 1; }
[[ -f "$OUT_DIR/blah.html" ]] || { echo "Missing built blah.html"; exit 1; }
[[ -f "$OUT_DIR/css/site.css" ]] || { echo "Missing copied CSS asset"; exit 1; }
[[ -f "$OUT_DIR/js/site.js" ]] || { echo "Missing copied JS asset"; exit 1; }
[[ ! -e "$OUT_DIR/components/home-callout.html" ]] || { echo "Component HTML should not be emitted"; exit 1; }

grep -q '<title>Welcome</title>' "$OUT_DIR/index.html" || {
  echo "Built index.html is missing the merged title slot"
  exit 1
}

grep -q 'class="home-callout"' "$OUT_DIR/index.html" || {
  echo "Built index.html is missing the included component wrapper"
  exit 1
}

grep -q 'This content is included from a static HTML component\.' "$OUT_DIR/index.html" || {
  echo "Built index.html is missing expanded include content"
  exit 1
}

grep -q 'This is the home page with all the latest updates\.' "$OUT_DIR/index.html" || {
  echo "Built index.html is missing merged main content"
  exit 1
}

echo "Sample build verification passed."
