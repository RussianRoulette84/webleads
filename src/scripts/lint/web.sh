# shellcheck shell=bash
# The page's own stages: stylesheets, scripts, shell and machine-readable files.
#
# None of the usual web tools are in this image, so each stage uses what is here
# and says plainly when the real tool is missing rather than passing by default.

_web_files() {
  find "$REPO/web" "$REPO/src/webleads/crates/ui" "$REPO/src/webleads/crates/server" \
    -type f -name "$1" -not -path "*/dist/*" -not -path "*/target/*" 2>/dev/null | sort
}

stage_css() {
  local hits
  # `!important` is the one rule a stylesheet here can break on its own: it is
  # allowed over third-party styling, and only with a reason written beside it.
  # Everything else about CSS needs stylelint, which this image has not got.
  hits=$(REPO="$REPO" python3 "$SCRIPT_DIR/lint/important.py")
  DETAIL_CSS="$hits"
  if [[ -z "$hits" ]]; then
    row "css" "ok" "${DIM}0 errors · 0 warnings${RESET}"
    return 0
  fi
  local n; n=$(printf '%s\n' "$hits" | grep -c . || true)
  row "css" "warn" "${YELLOW}$n warnings${RESET}"
  return 0
}

stage_js() {
  need_cmd node "js" || return 1
  local f out e=0
  DETAIL_JS=""
  while IFS= read -r f; do
    [[ -z "$f" ]] && continue
    out=$(node --check "$f" 2>&1) || {
      e=$(( e + 1 ))
      DETAIL_JS+="${f#"$REPO"/}: error: $(printf '%s' "$out" | sed -n 2p)"$'\n'
    }
  done < <( { _web_files "*.js"; _web_files "*.mjs"; } )
  if (( e == 0 )); then
    row "js" "ok" "${DIM}0 errors${RESET}"
    return 0
  fi
  row "js" "fail" "${RED}$e errors${RESET}"
  return 1
}

stage_bash() {
  local f out e=0 w=0
  DETAIL_BASH=""
  while IFS= read -r f; do
    [[ -z "$f" ]] && continue
    if command -v shellcheck >/dev/null 2>&1; then
      out=$(shellcheck -x --severity=warning --format=gcc "$f" 2>/dev/null || true)
      [[ -n "$out" ]] && DETAIL_BASH+="$(printf '%s\n' "$out" | sed "s|$REPO/||")"$'\n'
      e=$(( e + $(printf '%s\n' "$out" | grep -cE ': error: ' || true) ))
      w=$(( w + $(printf '%s\n' "$out" | grep -cE ': warning: ' || true) ))
    else
      out=$(bash -n "$f" 2>&1) || {
        e=$(( e + 1 ))
        DETAIL_BASH+="${f#"$REPO"/}: error: $out"$'\n'
      }
    fi
  done < <(find "$REPO/src/scripts" "$REPO/bin" -type f -name "*.sh" 2>/dev/null | sort)

  if ! command -v shellcheck >/dev/null 2>&1; then
    if (( e == 0 )); then
      row "bash" "warn" "${DIM}0 errors${RESET} ${YELLOW}· syntax only, shellcheck not installed${RESET}"
      return 0
    fi
    row "bash" "fail" "${RED}$e errors${RESET} ${DIM}· syntax only${RESET}"
    return 1
  fi
  if (( e == 0 )); then
    row "bash" "ok" "${DIM}0 errors · $w warnings${RESET}"
    return 0
  fi
  row "bash" "fail" "${RED}$e errors${RESET} ${DIM}· $w warnings${RESET}"
  return 1
}

stage_json() {
  local hits
  hits=$(REPO="$REPO" python3 <<'PY'
import json, os, pathlib, tomllib

root = pathlib.Path(os.environ["REPO"])
skip = {"target", "dist", "node_modules", ".git", ".Trash-0", "data", "logs"}
bad = []
for path in root.rglob("*"):
    if not path.is_file() or any(p in skip for p in path.parts):
        continue
    try:
        if path.suffix == ".json":
            json.loads(path.read_text(encoding="utf-8", errors="ignore"))
        elif path.suffix == ".toml":
            tomllib.loads(path.read_text(encoding="utf-8", errors="ignore"))
        else:
            continue
    except Exception as broke:
        bad.append(f"{path.relative_to(root)}: error: {broke}")
print("\n".join(bad))
PY
)
  DETAIL_JSON="$hits"
  if [[ -z "$hits" ]]; then
    row "json" "ok" "${DIM}0 errors${RESET}"
    return 0
  fi
  local n; n=$(printf '%s\n' "$hits" | grep -c . || true)
  row "json" "fail" "${RED}$n errors${RESET}"
  return 1
}
