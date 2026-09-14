#!/usr/bin/env python3
"""Check internal Markdown links in the Rust handbook."""

import re
import sys
from pathlib import Path

LINK_RE = re.compile(r"\[([^\]]*)\]\(([^)]+)\)")
EXCLUDED_SCHEMES = {"http", "https", "mailto"}


def main() -> int:
    book_dir = Path(__file__).resolve().parent.parent / "book"
    md_files = list(book_dir.rglob("*.md"))
    broken: list[tuple[Path, str, str]] = []

    for md_file in md_files:
        text = md_file.read_text(encoding="utf-8")
        for _, target in LINK_RE.findall(text):
            if any(target.startswith(scheme + ":") for scheme in EXCLUDED_SCHEMES):
                continue
            if target.startswith("#"):
                # In-page anchor; not validated here.
                continue

            # Split off anchor.
            path_part, _, _ = target.partition("#")
            if not path_part:
                continue

            resolved = (md_file.parent / path_part).resolve()
            if not resolved.exists():
                broken.append((md_file, target, str(resolved.relative_to(book_dir.resolve()))))

    if broken:
        print("Broken internal links:", file=sys.stderr)
        for source, target, resolved in broken:
            print(f"  {source.relative_to(book_dir)} -> {target} (resolved: {resolved})", file=sys.stderr)
        return 1

    print(f"Checked {len(md_files)} files; no broken internal links.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
