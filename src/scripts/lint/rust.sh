# shellcheck shell=bash
# The Rust stages: formatting, the compiler's own fault-finder, and the page.
#
# The page is built for WebAssembly, so it cannot ride along with the rest and is
# checked on its own.

CRATES="$REPO/src/webleads"

stage_fmt() {
  need_cmd cargo "fmt" || return 1
  local out
  out=$(cd "$CRATES" && cargo fmt --all --check 2>&1) || true
  if [[ -z "$out" ]]; then
    row "fmt" "ok" "${DIM}every file as rustfmt wants it${RESET}"
    return 0
  fi
  # Each file rustfmt would change announces itself with "Diff in <path>".
  local files n
  files=$(printf '%s\n' "$out" | grep -oE 'Diff in [^ ]+' | sed "s|Diff in $CRATES/||" | sort -u)
  n=$(printf '%s\n' "$files" | grep -c . || true)
  DETAIL_FMT=$(printf '%s\n' "$files" | sed 's/$/  E  not formatted/')
  row "fmt" "fail" "${RED}$n errors${RESET} ${DIM}· run: cargo fmt --all${RESET}"
  return 1
}

stage_clippy() {
  need_cmd cargo "clippy" || return 1
  _clippy "clippy" "$(printf '%s ' --workspace --exclude webleads-ui --all-targets)"
}

stage_page() {
  need_cmd cargo "page" || return 1
  _clippy "page" "$(printf '%s ' -p webleads-ui --target wasm32-unknown-unknown)"
}

# One clippy run, read back from its own machine-readable output so the counts and
# the listed faults can never disagree.
_clippy() {
  local label="$1" args="$2" out e w detail
  # shellcheck disable=SC2086  # the arguments are ours, and must word-split
  out=$(cd "$CRATES" && cargo clippy $args --message-format=short 2>&1) || true
  detail=$(printf '%s\n' "$out" | grep -E '^[^ ].*:[0-9]+:[0-9]+: (error|warning)' \
    | sed "s|$CRATES/||" | sort -u)
  e=$(printf '%s\n' "$detail" | grep -cE ': error: ' || true)
  w=$(printf '%s\n' "$detail" | grep -cE ': warning: ' || true)
  case "$label" in
    clippy) DETAIL_CLIPPY="$detail" ;;
    page)   DETAIL_PAGE="$detail" ;;
  esac
  if (( e == 0 && w == 0 )); then
    row "$label" "ok" "${DIM}0 errors · 0 warnings${RESET}"
    return 0
  fi
  if (( e == 0 )); then
    row "$label" "warn" "${YELLOW}$w warnings${RESET}"
    return 0
  fi
  row "$label" "fail" "${RED}$e errors${RESET} ${DIM}· $w warnings${RESET}"
  return 1
}
