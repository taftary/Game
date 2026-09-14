#!/usr/bin/env bash
set -euo pipefail

# Validate the Rust handbook: build it with the pinned mdbook version and check
# internal Markdown links.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

"${SCRIPT_DIR}/build-book.sh"

echo "Checking internal links..."
if python3 --version >/dev/null 2>&1; then
    python3 "${SCRIPT_DIR}/check-links.py"
else
    python "${SCRIPT_DIR}/check-links.py"
fi

echo "Checking required sections..."
if python3 --version >/dev/null 2>&1; then
    python3 "${SCRIPT_DIR}/check-sections.py"
else
    python "${SCRIPT_DIR}/check-sections.py"
fi
