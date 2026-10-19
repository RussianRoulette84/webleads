# shellcheck shell=bash
# The rows, the closing line and the detail blocks. Sourced by lint.sh.

# Running totals for the closing line. Each stage prints its own count, and the
# tally is read back off the row it printed — so no stage has to report twice.
LINT_ERR=0
LINT_WARN=0

row() {
  local name="$1" state="$2" msg="$3" mark col acc
  acc="$(get_accent)"
  case "$state" in
    ok)   mark="✓"; col="$GREEN" ;;
    fail) mark="✗"; col="$RED" ;;
    warn) mark="!"; col="$YELLOW" ;;
  esac
  if [[ "$msg" =~ ([0-9]+)[[:space:]]+errors? ]];   then LINT_ERR=$(( LINT_ERR + BASH_REMATCH[1] )); fi
  if [[ "$msg" =~ ([0-9]+)[[:space:]]+warnings? ]]; then LINT_WARN=$(( LINT_WARN + BASH_REMATCH[1] )); fi
  printf "%b%s%s%b%b%-11s%b %b%s%b %b\n" \
    "$acc" "$TREE_MID" "$DIAMOND_EMPTY" "$RESET" \
    "$BOLD" "$name" "$RESET" \
    "$col" "$mark" "$RESET" \
    "$msg"
}

section_done() {
  local col es ws
  (( LINT_ERR == 0 )) && col="$GREEN" || col="$RED"
  (( LINT_ERR == 1 )) && es="" || es="s"
  (( LINT_WARN == 1 )) && ws="" || ws="s"
  printf "%b%s%s%b%b%s%b — %b%d error%s, %d warning%s%b\n\n" \
    "$col" "$TREE_BOT" "$DIAMOND_EMPTY" "$RESET" "$BOLD" "Done" "$RESET" \
    "$col" "$LINT_ERR" "$es" "$LINT_WARN" "$ws" "$RESET"
}

# Colour a finding: errors red, warnings yellow, anything quieter dim.
_detail_line() {
  local line="$1" lc
  lc="$(get_accent)"
  if   [[ "$line" == *"  E  "* || "$line" == *": error: "* ]]; then lc="$RED"
  elif [[ "$line" == *"  W  "* || "$line" == *": warning: "* ]]; then lc="$YELLOW"
  elif [[ "$line" == *": note: "* ]]; then lc="$DIM"
  fi
  printf "%b%s%b%s\n" "$lc" "$TREE_BAR" "$RESET" "$line"
}

# What a stage found, under the summary rather than bursting out of it.
detail_block() {
  local title="$1" body="$2" col line
  [[ -z "$body" ]] && return
  if   [[ "$body" == *"  E  "* || "$body" == *": error: "* ]]; then col="$RED"
  elif [[ "$body" == *"  W  "* || "$body" == *": warning: "* ]]; then col="$YELLOW"
  else col="$(get_accent)"
  fi
  printf "%b%s%s%b%b%s%b\n" "$col" "$TREE_TOP" "$DIAMOND_FILLED" "$RESET" "$BOLD" "$title" "$RESET"
  while IFS= read -r line; do
    [[ -z "$line" ]] && continue
    _detail_line "$line"
  done <<< "$body"
  printf "%b%s%b\n\n" "$col" "$TREE_BOT" "$RESET"
}

# A tool this image does not carry. Said out loud and counted as an error, never
# quietly skipped: a clean run that checked nothing is the worst answer of all.
need_cmd() {
  local bin="$1" label="$2"
  command -v "$bin" >/dev/null 2>&1 && return 0
  row "$label" "fail" "${RED}not installed${RESET} ${DIM}— ask for it in the image${RESET}"
  return 1
}
