# Documentation Scripts

Automation for the Rust handbook lives here. `check-book.sh` builds the book
with the pinned mdBook version (`build-book.sh`, pinned via
`docs/.mdbook-version`), then validates internal Markdown links
(`check-links.py`) and required page sections (`check-sections.py`). Scripts
must not modify production source files.
