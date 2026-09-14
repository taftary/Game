#!/usr/bin/env bash
set -euo pipefail

# Reproducible mdBook build for the Rust handbook.
# Reads the pinned version from docs/.mdbook-version and installs it
# via cargo if it is not already present.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DOCS_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
PROJECT_ROOT="$(cd "${DOCS_DIR}/.." && pwd)"

MDBOOK_VERSION="$(cat "${DOCS_DIR}/.mdbook-version" | tr -d '[:space:]')"
MDBOOK_INSTALL_DIR="${PROJECT_ROOT}/target/mdbook"
MDBOOK_BIN="${MDBOOK_INSTALL_DIR}/bin/mdbook"

install_mdbook() {
    echo "Installing mdbook ${MDBOOK_VERSION}..."
    cargo install mdbook \
        --version "${MDBOOK_VERSION}" \
        --root "${MDBOOK_INSTALL_DIR}" \
        --locked
}

if [[ ! -x "${MDBOOK_BIN}" ]]; then
    install_mdbook
else
    INSTALLED_VERSION="$(${MDBOOK_BIN} --version | awk '{print $2}' | sed 's/^v//')"
    if [[ "${INSTALLED_VERSION}" != "${MDBOOK_VERSION}" ]]; then
        echo "mdbook version mismatch: found ${INSTALLED_VERSION}, want ${MDBOOK_VERSION}"
        install_mdbook
    fi
fi

echo "Building Rust handbook with mdbook ${MDBOOK_VERSION}..."
"${MDBOOK_BIN}" build "${DOCS_DIR}"
