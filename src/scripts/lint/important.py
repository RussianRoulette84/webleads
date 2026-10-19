"""Find `!important` that nobody explained.

The rule is not "never": it is allowed over styling we cannot change, scoped to one
property, with the reason written beside it. So a finding is an `!important` with no
comment in the three lines above it. Comments are blanked out first, line for line,
so a comment that merely mentions the word is not itself a finding.
"""

import os
import pathlib
import re

ROOT = pathlib.Path(os.environ["REPO"])
PLACES = ["web", "src/webleads/crates/ui", "src/webleads/crates/server"]
COMMENT = re.compile(r"/\*.*?\*/", re.S)


def blanked(text: str) -> str:
    """The same text with every comment turned into spaces, line count intact."""
    return COMMENT.sub(lambda m: re.sub(r"[^\n]", " ", m.group(0)), text)


def findings(text: str, name: str) -> list[str]:
    """Every unexplained `!important` in one file, as a line you can read."""
    raw = text.split("\n")
    bare = blanked(text).split("\n")
    out = []
    for n, line in enumerate(bare, 1):
        if "!important" not in line:
            continue
        if any("/*" in before for before in raw[max(0, n - 4):n - 1]):
            continue
        out.append(f"{name}:{n}  W  !important with no reason beside it")
    return out


def main() -> None:
    said = []
    for place in PLACES:
        here = ROOT / place
        for path in sorted(list(here.rglob("*.css")) + list(here.rglob("*.html"))):
            if "dist" in path.parts or "target" in path.parts:
                continue
            text = path.read_text(encoding="utf-8", errors="ignore")
            said += findings(text, str(path.relative_to(ROOT)))
    print("\n".join(said))


if __name__ == "__main__":
    main()
