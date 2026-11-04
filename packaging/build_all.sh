#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

cd "${ROOT_DIR}"

echo "==> Removing obsolete SCX files and artifacts..."
rm -rf "${ROOT_DIR}/src/kernel/scx.rs" \
       "${ROOT_DIR}/packaging/build_scx_packages.sh" \
       "${ROOT_DIR}/data/scx" \
       "${ROOT_DIR}/dist"

chmod +x "${ROOT_DIR}/packaging/build_flatpak.sh" \
         "${ROOT_DIR}/packaging/build_deb.sh" \
         "${ROOT_DIR}/packaging/build_appimage.sh" \
         "${ROOT_DIR}/packaging/publish_github.sh"

if [[ ! -f "${ROOT_DIR}/target/release/mithrandir" ]]; then
  echo "==> Building release binary (v1.1.2, GUI enabled) inside dev-fedora container..."
  podman run --rm --userns=keep-id \
    -v mithrandir-cargo-cache:/var/tmp/cargo:Z -e CARGO_HOME=/var/tmp/cargo \
    -v "${ROOT_DIR}:/workspace:Z" -w /workspace localhost/dev-fedora \
    cargo build --release --features gui
fi

echo "==> Building Flatpak bundle..."
"${ROOT_DIR}/packaging/build_flatpak.sh"

echo "==> Building Debian package..."
"${ROOT_DIR}/packaging/build_deb.sh"

echo "==> Building AppImage..."
"${ROOT_DIR}/packaging/build_appimage.sh"

echo "==> Publishing v1.1.2 to GitHub..."
"${ROOT_DIR}/packaging/publish_github.sh"

echo "==> All v1.1.2 packages built and published successfully!"
ls -lh "${ROOT_DIR}"/mithrandir*1.1.2*
