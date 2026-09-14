#!/usr/bin/env python3
"""Check that handbook pages contain their required sections.

Sections are recognized as either `## Heading` Markdown headings or as bold
paragraph markers like `**Summary**` at the start of a line.
"""

import re
import sys
from pathlib import Path

HEADING_RE = re.compile(r"^#{2,3}\s+(.+)$", re.MULTILINE)
BOLD_MARKER_RE = re.compile(r"^\*\*([^*]+)\*\*", re.MULTILINE)


class Requirement:
    """One or more section names, any of which satisfies the requirement."""

    def __init__(self, *alternatives: str):
        self.alternatives = set(alternatives)

    def satisfied_by(self, sections: set[str]) -> bool:
        return not self.alternatives.isdisjoint(sections)

    def __str__(self) -> str:
        if len(self.alternatives) == 1:
            return next(iter(self.alternatives))
        return " or ".join(sorted(self.alternatives))


REQUIRED_BY_CATEGORY: dict[str, list[Requirement]] = {
    "architecture": [
        Requirement("summary", "overview"),
        Requirement("key points", "rules", "use", "avoid"),
    ],
    "principles": [
        Requirement("summary", "overview"),
        Requirement("key points", "rules", "use", "avoid"),
    ],
    "patterns": [
        Requirement("summary", "overview"),
        Requirement("example", "minimal example"),
        Requirement("key points", "rules", "use", "avoid"),
    ],
    "practices": [
        Requirement("summary", "overview"),
        Requirement("key points", "rules", "use", "avoid"),
    ],
    "decisions": [
        Requirement("context"),
        Requirement("decision"),
        Requirement("consequences"),
    ],
    "examples": [Requirement("summary", "overview")],
    "references": [],
}


def category(md_file: Path) -> str:
    parts = md_file.parts
    try:
        idx = parts.index("book")
        sub = parts[idx + 1]
        # ADRs live under architecture/decisions/ but should use decision rules.
        if sub == "architecture" and len(parts) > idx + 2 and parts[idx + 2] == "decisions":
            return "decisions"
        return sub
    except (ValueError, IndexError):
        return ""


def normalized_sections(text: str) -> set[str]:
    headings = {m.group(1).strip().lower() for m in HEADING_RE.finditer(text)}
    bold = {m.group(1).strip().lower() for m in BOLD_MARKER_RE.finditer(text)}
    return headings | bold


def check_page(md_file: Path) -> list[str]:
    cat = category(md_file)
    requirements = REQUIRED_BY_CATEGORY.get(cat)
    if requirements is None:
        return []

    text = md_file.read_text(encoding="utf-8")
    sections = normalized_sections(text)
    missing = [str(req) for req in requirements if not req.satisfied_by(sections)]
    return missing


def main() -> int:
    book_dir = Path(__file__).resolve().parent.parent / "book"
    md_files = sorted(book_dir.rglob("*.md"))
    failures: list[tuple[Path, list[str]]] = []

    for md_file in md_files:
        missing = check_page(md_file)
        if missing:
            failures.append((md_file, missing))

    if failures:
        print("Pages missing required sections:", file=sys.stderr)
        for md_file, missing in failures:
            rel = md_file.relative_to(book_dir)
            print(f"  {rel}: {', '.join(missing)}", file=sys.stderr)
        return 1

    print(f"Checked {len(md_files)} pages; all required sections present.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
