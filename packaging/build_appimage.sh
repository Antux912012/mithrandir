#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

VERSION=$(grep -m1 '^version' "${ROOT_DIR}/Cargo.toml" | sed -E 's/.*"([^"]+)".*/\1/')
APP_ID="io.github.antux912012.Mithrandir"
APP_NAME="mithrandir"
OUTPUT_APPIMAGE="${ROOT_DIR}/${APP_NAME}-${VERSION}-x86_64.AppImage"
APPDIR="${ROOT_DIR}/target/pkg-appimage/AppDir"

echo "==> Building Mithrandir AppImage (v${VERSION})..."

# 1. Ensure binary exists
if [[ ! -f "${ROOT_DIR}/target/release/mithrandir" ]]; then
  echo "==> Compiling release binary..."
  podman run --rm --userns=keep-id \
    -v mithrandir-cargo-cache:/var/tmp/cargo:Z -e CARGO_HOME=/var/tmp/cargo \
    -v "${ROOT_DIR}:/workspace:Z" -w /workspace localhost/dev-fedora \
    cargo build --release --features gui
fi

# 2. Stage AppDir
rm -rf "${APPDIR}"
mkdir -p "${APPDIR}/usr/bin"
mkdir -p "${APPDIR}/usr/share/applications"
mkdir -p "${APPDIR}/usr/share/glib-2.0/schemas"
mkdir -p "${APPDIR}/usr/share/metainfo"
mkdir -p "${APPDIR}/usr/share/icons/hicolor"

install -m 755 "${ROOT_DIR}/target/release/mithrandir" "${APPDIR}/usr/bin/mithrandir"
install -m 644 "${ROOT_DIR}/data/${APP_ID}.desktop" "${APPDIR}/usr/share/applications/${APP_ID}.desktop"
install -m 644 "${ROOT_DIR}/data/${APP_ID}.gschema.xml" "${APPDIR}/usr/share/glib-2.0/schemas/${APP_ID}.gschema.xml"
install -m 644 "${ROOT_DIR}/data/${APP_ID}.metainfo.xml" "${APPDIR}/usr/share/metainfo/${APP_ID}.metainfo.xml"
cp -r "${ROOT_DIR}/data/icons/hicolor/"* "${APPDIR}/usr/share/icons/hicolor/"

# Compile gsettings schemas
glib-compile-schemas "${APPDIR}/usr/share/glib-2.0/schemas"

# Root level desktop file and icon
cp "${ROOT_DIR}/data/${APP_ID}.desktop" "${APPDIR}/${APP_ID}.desktop"
cp "${ROOT_DIR}/data/icons/hicolor/256x256/apps/${APP_ID}.png" "${APPDIR}/${APP_ID}.png"
cp "${ROOT_DIR}/data/icons/hicolor/256x256/apps/${APP_ID}.png" "${APPDIR}/.DirIcon"

# AppRun launcher script
cat > "${APPDIR}/AppRun" << 'EOF'
#!/bin/sh
set -e

HERE="$(dirname "$(readlink -f "${0}")")"
export PATH="${HERE}/usr/bin:${PATH}"
export XDG_DATA_DIRS="${HERE}/usr/share:${XDG_DATA_DIRS:-/usr/local/share:/usr/share}"
export GSETTINGS_SCHEMA_DIR="${HERE}/usr/share/glib-2.0/schemas:${GSETTINGS_SCHEMA_DIR}"

exec "${HERE}/usr/bin/mithrandir" "$@"
EOF
chmod +x "${APPDIR}/AppRun"

# 3. Package AppImage using appimagetool
TOOL_DIR="${ROOT_DIR}/target/tools"
if [[ ! -d "${TOOL_DIR}/squashfs-root" ]]; then
  echo "==> Fetching appimagetool..."
  mkdir -p "${TOOL_DIR}"
  cd "${TOOL_DIR}"
  curl -sSL -o appimagetool.AppImage https://github.com/AppImage/AppImageKit/releases/download/continuous/appimagetool-x86_64.AppImage
  chmod +x appimagetool.AppImage
  ./appimagetool.AppImage --appimage-extract >/dev/null
  cd "${ROOT_DIR}"
fi

echo "==> Generating AppImage with appimagetool..."
ARCH=x86_64 "${TOOL_DIR}/squashfs-root/AppRun" "${APPDIR}" "${OUTPUT_APPIMAGE}"

echo "==> AppImage successfully generated at:"
echo "    ${OUTPUT_APPIMAGE}"
