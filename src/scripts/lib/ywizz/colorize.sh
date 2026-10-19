# shellcheck shell=bash
# Prints a block of text in the accent colour, for the banner.

colorize() {
  local acc line
  acc="$(get_accent)"
  while IFS= read -r line; do
    printf "%b%s%b\n" "$acc" "$line" "$RESET"
  done
}
