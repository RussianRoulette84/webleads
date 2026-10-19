#!/usr/bin/env bash
# Run the test suite and report how much of the code it reaches.
#
# Use `./scripts/test.sh` for the numbers, `./scripts/test.sh --fast` to skip the
# coverage pass when you just want to know whether it still works.
set -euo pipefail

cd "$(dirname "$0")/../webleads"

# Builds go to the project's own target/, which is where cargo puts them by default
# and what .gitignore already covers. It survives a reboot; /tmp does not.
export PATH="$HOME/.cargo/bin:$PATH"

# The web page is built for WebAssembly, so it is kept out of the native runs and
# checked on its own below. Its *pure* parts — the wording, the sliders' ranges, the
# pin colours — do compile natively, and their tests run in the "page" step. They
# were silently never running before.
NATIVE=(--workspace --exclude webleads-ui)

say() { printf '%b\n' "\033[1;35m=== $1 ===\033[0m"; }

say "format"
cargo fmt --all --check

say "clippy"
cargo clippy "${NATIVE[@]}" --all-targets -- -D warnings

say "page"
cargo clippy -p webleads-ui --target wasm32-unknown-unknown -- -D warnings
cargo nextest run -p webleads-ui

if [[ "${1:-}" == "--fast" ]]; then
  say "tests"
  cargo nextest run "${NATIVE[@]}"
  exit 0
fi

say "tests + coverage"
# nextest is quicker and its output is far easier to read than cargo test's.
cargo llvm-cov nextest "${NATIVE[@]}" --summary-only

printf '%b\n' "\033[1;33mRead the number per file, not as one figure.\033[0m"
printf '%s\n' "Functions that are only I/O — the database writes, the fetch inside" \
              "check_one, main — are covered by the integration layer, not by unit" \
              "tests. A low number on those is expected, not a failure."
