# shellcheck shell=bash
# The house colours and the tree glyphs every script in here draws with.
#
# Purple is the accent and the only strong colour on a clean run; green, red and
# yellow belong to a result, never to decoration. Everything goes plain under
# NO_COLOR or when the output is not a terminal, so a log file stays readable.

# The box walls are drawn by counting characters, so the shell has to be told that
# "·" and "─" are one character each rather than three bytes.
export LC_ALL="${LC_ALL:-C.UTF-8}"

if [[ -n "${NO_COLOR:-}" ]] || [[ ! -t 1 ]]; then
  BOLD=""; DIM=""; RESET=""
  RED=""; GREEN=""; YELLOW=""; C7=""
else
  BOLD=$'\033[1m'; DIM=$'\033[2m'; RESET=$'\033[0m'
  RED=$'\033[1;31m'; GREEN=$'\033[1;32m'; YELLOW=$'\033[1;33m'
  C7=$'\033[1;35m'
fi

TREE_TOP="├ "
TREE_MID="├ "
TREE_BAR="│ "
TREE_BOT="╰ "
DIAMOND_EMPTY="◇ "
DIAMOND_FILLED="◆ "

export BOLD DIM RESET RED GREEN YELLOW C7
export TREE_TOP TREE_MID TREE_BAR TREE_BOT DIAMOND_EMPTY DIAMOND_FILLED

# The accent, asked for by name so a script can set its own without editing this.
get_accent() { printf '%s' "${accent_color:-$C7}"; }

# How wide to draw, read fresh each time so a resized window is respected.
term_width() {
  local w
  w=$(tput cols 2>/dev/null || echo 80)
  [[ "$w" =~ ^[0-9]+$ ]] || w=80
  (( w > 100 )) && w=100
  (( w < 40 )) && w=40
  printf '%s' "$w"
}
