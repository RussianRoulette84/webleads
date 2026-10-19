# shellcheck shell=bash
# How long each file has got. 300 lines is the sweet spot, 500 the cap, and the
# one page that holds the whole stylesheet is allowed more.

stage_size() {
  local -a wlpat=() over=() biggest=()
  local line f len rel cap
  if [[ -f "$WHITELIST" ]]; then
    while IFS= read -r line; do
      line="${line%%#*}"; line="${line## }"; line="${line%% }"
      [[ -n "$line" ]] && wlpat+=("$line")
    done < "$WHITELIST"
  fi
  whitelisted() {
    local p
    # shellcheck disable=SC2053  # the pattern is meant to glob
    for p in "${wlpat[@]}"; do [[ "$1" == $p ]] && return 0; done
    return 1
  }

  while IFS= read -r f; do
    [[ -z "$f" ]] && continue
    len=$(wc -l < "$f")
    rel="${f#"$REPO"/}"
    # A page holding the whole stylesheet and a file of test cases are both lists
    # rather than logic, and the house rule exempts a list.
    if   [[ "$rel" == *.html ]];     then cap="$SIZE_CAP_HTML"
    elif [[ "$rel" == *_tests.rs ]]; then cap="$SIZE_CAP_TESTS"
    else cap="$SIZE_CAP"; fi
    biggest+=("$len|$rel")
    (( len > cap )) || continue
    whitelisted "$rel" && continue
    over+=("$len|$rel|$cap")
  done < <(find "$REPO/src" "$REPO/web" "$REPO/bin" -type f \
    \( -name "*.rs" -o -name "*.sh" -o -name "*.js" -o -name "*.mjs" \
       -o -name "*.css" -o -name "*.html" -o -name "*.sql" \) \
    -not -path "*/target/*" -not -path "*/dist/*" -not -path "*/node_modules/*" \
    -not -path "*/.Trash-*/*" 2>/dev/null)

  DETAIL_SIZE=$(printf '%s\n' "${over[@]}" | sort -t'|' -k1 -rn \
    | awk -F'|' '$2 != "" { printf "%s  W  %s lines, over %s\n", $2, $1, $3 }')
  SIZE_BIGGEST=$(printf '%s\n' "${biggest[@]}" | sort -t'|' -k1 -rn | head -n "$SIZE_TOP")

  local n=${#over[@]} s
  (( n == 1 )) && s="" || s="s"
  if (( n == 0 )); then
    row "size" "ok" "${DIM}0 over cap (code $SIZE_CAP · tests $SIZE_CAP_TESTS · page $SIZE_CAP_HTML)${RESET}"
    return 0
  fi
  if (( STRICT )); then
    row "size" "fail" "${RED}$n errors${RESET} ${DIM}· file$s over cap${RESET}"
    return 1
  fi
  row "size" "warn" "${YELLOW}$n warnings${RESET} ${DIM}· file$s over cap, --strict to fail${RESET}"
  return 0
}

# The longest files there are, cap or no cap — the list you read when deciding
# what to split next.
size_biggest() {
  local acc; acc="$(get_accent)"
  [[ -z "$SIZE_BIGGEST" ]] && return
  printf "%b%s%s%b%b%s%b %b(top %s)%b\n" \
    "$acc" "$TREE_TOP" "$DIAMOND_FILLED" "$RESET" "$BOLD" "longest files" "$RESET" \
    "$DIM" "$SIZE_TOP" "$RESET"
  printf '%s\n' "$SIZE_BIGGEST" | awk -F'|' -v acc="$acc" -v bar="$TREE_BAR" -v r="$RESET" \
    -v d="$DIM" '{ printf "%s%s%s%5d  %s%s%s\n", acc, bar, r, $1, d, $2, r }'
  printf "%b%s%b\n\n" "$acc" "$TREE_BOT" "$RESET"
}
