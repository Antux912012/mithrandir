#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

cd "${ROOT_DIR}"

VERSION=$(grep -m1 '^version' "${ROOT_DIR}/Cargo.toml" | sed -E 's/.*"([^"]+)".*/\1/')
APP_ID="io.github.antux912012.Mithrandir"
DEB_NAME="mithrandir_${VERSION}_amd64.deb"
PKG_DIR="${ROOT_DIR}/target/pkg-deb"

echo "==> Building Mithrandir Debian package (${DEB_NAME})..."

if [[ ! -f "${ROOT_DIR}/target/release/mithrandir" ]]; then
  echo "==> Compiling release binary..."
  podman run --rm --userns=keep-id \
    -v mithrandir-cargo-cache:/var/tmp/cargo:Z -e CARGO_HOME=/var/tmp/cargo \
    -v "${ROOT_DIR}:/workspace:Z" -w /workspace localhost/dev-fedora \
    cargo build --release --features gui
fi

rm -rf "${PKG_DIR}"
mkdir -p "${PKG_DIR}/DEBIAN"
mkdir -p "${PKG_DIR}/usr/bin"
mkdir -p "${PKG_DIR}/usr/share/applications"
mkdir -p "${PKG_DIR}/usr/share/glib-2.0/schemas"
mkdir -p "${PKG_DIR}/usr/share/metainfo"
mkdir -p "${PKG_DIR}/usr/share/polkit-1/actions"
mkdir -p "${PKG_DIR}/usr/lib/udev/rules.d"
mkdir -p "${PKG_DIR}/usr/share/icons/hicolor"

install -m 755 "${ROOT_DIR}/target/release/mithrandir" "${PKG_DIR}/usr/bin/mithrandir"
install -m 644 "${ROOT_DIR}/data/${APP_ID}.desktop" "${PKG_DIR}/usr/share/applications/${APP_ID}.desktop"
install -m 644 "${ROOT_DIR}/data/${APP_ID}.gschema.xml" "${PKG_DIR}/usr/share/glib-2.0/schemas/${APP_ID}.gschema.xml"
install -m 644 "${ROOT_DIR}/data/${APP_ID}.metainfo.xml" "${PKG_DIR}/usr/share/metainfo/${APP_ID}.metainfo.xml"
install -m 644 "${ROOT_DIR}/data/${APP_ID}.policy" "${PKG_DIR}/usr/share/polkit-1/actions/${APP_ID}.policy"
install -m 644 "${ROOT_DIR}/data/99-mithrandir-power.rules" "${PKG_DIR}/usr/lib/udev/rules.d/99-mithrandir-power.rules"
cp -r "${ROOT_DIR}/data/icons/hicolor/"* "${PKG_DIR}/usr/share/icons/hicolor/"

cat > "${PKG_DIR}/DEBIAN/control" <<EOF
Package: mithrandir
Version: ${VERSION}
Section: utils
Priority: optional
Architecture: amd64
Maintainer: Antonio <antux912012@users.noreply.github.com>
Depends: libgtk-4-1, libadwaita-1-0, libglib2.0-0, policykit-1 | pkexec, hicolor-icon-theme
Homepage: https://github.com/Antux912012/mithrandir
Description: Native GTK4 & Libadwaita GNU/Linux System Optimizer & Cleaner
 Mithrandir is a high-performance system maintenance suite, deep cache cleaner,
 BORE scheduler manager, and hardware power utility written in Rust.
EOF

podman run --rm --userns=keep-id \
  -v "${ROOT_DIR}:/workspace:Z" -w /workspace localhost/dev-debian \
  dpkg-deb --build --root-owner-group "target/pkg-deb" "${DEB_NAME}"

echo "==> Debian package generated successfully at:"
echo "    ${ROOT_DIR}/${DEB_NAME}"
