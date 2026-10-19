#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${ROOT_DIR}"

VERSION=$(grep -m1 "version:" "${ROOT_DIR}/meson.build" | sed -E "s/.*'([^']+)'.*/\1/")
PKG_DIR="${ROOT_DIR}/_build_deb_stage"
BUILD_DIR="${ROOT_DIR}/_build_deb_meson"

echo "==> Building Mithrandir v${VERSION} with Meson..."
rm -rf "${PKG_DIR}" "${BUILD_DIR}"
meson setup "${BUILD_DIR}" --prefix=/usr
meson compile -C "${BUILD_DIR}"
DESTDIR="${PKG_DIR}" meson install -C "${BUILD_DIR}" --no-rebuild

mkdir -p "${PKG_DIR}/DEBIAN"
cp "${ROOT_DIR}/packaging/debian/control" "${PKG_DIR}/DEBIAN/control"
sed -i "s/^Version:.*/Version: ${VERSION}/" "${PKG_DIR}/DEBIAN/control"
cp "${ROOT_DIR}/packaging/debian/postinst" "${PKG_DIR}/DEBIAN/postinst"
cp "${ROOT_DIR}/packaging/debian/postrm" "${PKG_DIR}/DEBIAN/postrm"
chmod 755 "${PKG_DIR}/DEBIAN/postinst" "${PKG_DIR}/DEBIAN/postrm"

DEB_PATH="${ROOT_DIR}/mithrandir_${VERSION}_all.deb"
dpkg-deb --build --root-owner-group "${PKG_DIR}" "${DEB_PATH}"
rm -rf "${PKG_DIR}" "${BUILD_DIR}"
echo "==> Built ${DEB_PATH}"
