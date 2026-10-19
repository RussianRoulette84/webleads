# shellcheck shell=bash
# The box at the top of a run: what this is, and what it looks at.

# A full-width box whose right wall lines up whatever the window is doing.
header_tui() {
  local title="$1" subtitle="${2:-}" acc width
  acc="$(get_accent)"
  width="$(term_width)"

  # Four visible characters open the line, one closes it: the dashes make up the
  # rest, so the right wall lands in the same column on every line.
  printf "%b%s%s%s╮%b\n" "$acc" "$TREE_TOP" "$DIAMOND_FILLED" \
    "$(printf '─%.0s' $(seq 1 $(( width - 5 ))))" "$RESET"
  _header_line "$title" "$width" "$acc" "$BOLD"
  [[ -n "$subtitle" ]] && _header_line "$subtitle" "$width" "$acc" "$DIM"
  printf "%b%s%s╯%b\n" "$acc" "$TREE_MID" \
    "$(printf '─%.0s' $(seq 1 $(( width - 3 ))))" "$RESET"
}

# One line inside the box, cut to fit rather than breaking the right wall.
_header_line() {
  local text="$1" width="$2" acc="$3" style="$4" room=$(( $2 - 4 ))
  (( ${#text} > room )) && text="${text:0:$(( room - 3 ))}..."
  printf "%b│%b %b%-*s%b %b│%b\n" \
    "$acc" "$RESET" "$style" "$room" "$text" "$RESET" "$acc" "$RESET"
}
