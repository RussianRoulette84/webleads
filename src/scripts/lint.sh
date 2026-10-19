#!/usr/bin/env bash
# The linter: formatting, clippy, the page, stylesheets, scripts and file length.
#
# Everything here is a check that takes seconds. The tests live in test.sh and are
# run for what you changed, not as a matter of course.
#
# Usage: lint.sh [help] [--strict] [--summary-only] [--top N] [--no-color]
#               [--only fmt clippy ...] [--exclude size ...]
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
REPO="$(cd "$SCRIPT_DIR/../.." && pwd -P)"
WHITELIST="$SCRIPT_DIR/.size-whitelist"
SIZE_CAP=500
SIZE_CAP_HTML=1500
SIZE_CAP_TESTS=900
SIZE_TOP=5
SUMMARY_ONLY=0
STRICT=0
LINT_HELP=0
export REPO SCRIPT_DIR WHITELIST SIZE_CAP SIZE_CAP_HTML SIZE_CAP_TESTS STRICT
export PATH="$HOME/.cargo/bin:$PATH"
[ -n "${NO_COLOR:-}" ] && export NO_COLOR=1

ALL_STAGES="fmt clippy page css js bash json size"

usage() {
  cat <<'EOF'
lint — formatting · clippy · the page · CSS · JS · shell · JSON · file length

Usage: ./src/scripts/lint.sh [option]...

  help              this message
  --strict          files over the length cap fail rather than warn
  --summary-only    the rows only, no banner and no detail blocks
  --top N           how many of the longest files to list (default 5)
  --only A B ..     run only these checks
  --exclude A B ..  skip these checks
  --no-color        plain output (or export NO_COLOR=1)

Checks: fmt clippy page css js bash json size

  ./src/scripts/lint.sh --only fmt clippy      just the Rust I touched
  ./src/scripts/lint.sh --exclude size         everything but the length cap

Lists errors and warnings. Only errors fail the gate.
EOF
}

SKIP=""
ONLY=""
take_top=0
collecting=""
for arg in "$@"; do
  if (( take_top )); then SIZE_TOP="$arg"; take_top=0; continue; fi
  # A list keeps eating words until the next --flag, so `--only fmt clippy` works
  # without quotes or commas.
  if [ -n "$collecting" ] && [ "${arg#--}" = "$arg" ]; then
    case " $ALL_STAGES " in
      *" $arg "*) [ "$collecting" = skip ] && SKIP="$SKIP $arg" || ONLY="$ONLY $arg" ;;
      *) echo "lint: no check called '$arg' — pick from: $ALL_STAGES" >&2; exit 2 ;;
    esac
    continue
  fi
  collecting=""
  case "$arg" in
    --summary-only)   SUMMARY_ONLY=1 ;;
    --no-color)       export NO_COLOR=1 ;;
    --strict)         STRICT=1 ;;
    --top)            take_top=1 ;;
    --top=*)          SIZE_TOP="${arg#*=}" ;;
    --exclude|--skip) collecting=skip ;;
    --only)           collecting=only ;;
    help|-h|--help)   LINT_HELP=1 ;;
  esac
done
[[ "$SIZE_TOP" =~ ^[0-9]+$ ]] && (( SIZE_TOP > 0 )) || SIZE_TOP=5
(( LINT_HELP )) && { usage; exit 0; }

# shellcheck source=src/scripts/lib/ywizz/theme.sh
source "$REPO/src/scripts/lib/ywizz/theme.sh"
export accent_color="$C7"
# shellcheck source=src/scripts/lib/ywizz/header.sh
source "$REPO/src/scripts/lib/ywizz/header.sh"

# shellcheck source=src/scripts/lint/ui.sh
source "$SCRIPT_DIR/lint/ui.sh"
# shellcheck source=src/scripts/lint/rust.sh
source "$SCRIPT_DIR/lint/rust.sh"
# shellcheck source=src/scripts/lint/web.sh
source "$SCRIPT_DIR/lint/web.sh"
# shellcheck source=src/scripts/lint/size.sh
source "$SCRIPT_DIR/lint/size.sh"

DETAIL_FMT=""; DETAIL_CLIPPY=""; DETAIL_PAGE=""
DETAIL_CSS=""; DETAIL_JS=""; DETAIL_BASH=""; DETAIL_JSON=""
DETAIL_SIZE=""; SIZE_BIGGEST=""

header_tui "Linter" "formatting · clippy · the page · CSS · JS · shell · JSON · length (max $SIZE_CAP, tests $SIZE_CAP_TESTS, page $SIZE_CAP_HTML)"

wanted() {
  [ -n "$ONLY" ] && { case " $ONLY " in *" $1 "*) return 0 ;; *) return 1 ;; esac; }
  case " $SKIP " in *" $1 "*) return 1 ;; *) return 0 ;; esac
}

ISSUES=0
SKIPPED=""
for st in $ALL_STAGES; do
  if wanted "$st"; then
    "stage_$st" || ISSUES=$(( ISSUES + 1 ))
  else
    SKIPPED="$SKIPPED $st"
  fi
done
# Say what was not looked at. A clean result that quietly skipped half the checks
# is worse than no result at all.
[ -n "$SKIPPED" ] && row "skipped" "warn" "${DIM}not checked:${SKIPPED}${RESET}"
section_done

if (( ! SUMMARY_ONLY )); then
  wanted size && size_biggest
  wanted fmt    && detail_block "fmt"    "$DETAIL_FMT"
  wanted clippy && detail_block "clippy" "$DETAIL_CLIPPY"
  wanted page   && detail_block "page"   "$DETAIL_PAGE"
  wanted css    && detail_block "css"    "$DETAIL_CSS"
  wanted js     && detail_block "js"     "$DETAIL_JS"
  wanted bash   && detail_block "bash"   "$DETAIL_BASH"
  wanted json   && detail_block "json"   "$DETAIL_JSON"
  wanted size   && detail_block "size"   "$DETAIL_SIZE"
fi

exit $(( ISSUES > 0 ? 1 : 0 ))
