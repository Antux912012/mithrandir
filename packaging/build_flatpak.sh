#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

cd "${ROOT_DIR}"

VERSION=$(grep -m1 '^version' "${ROOT_DIR}/Cargo.toml" | sed -E 's/.*"([^"]+)".*/\1/')
APP_ID="io.github.antonio.Mithrandir"
BUNDLE_PATH="${ROOT_DIR}/mithrandir_${VERSION}.flatpak"

echo "==> Staging Mithrandir Flatpak build (v${VERSION})..."

# Ensure release binary exists
if [[ ! -f "${ROOT_DIR}/target/release/mithrandir" ]]; then
  echo "==> Building Release Binary with GTK4 and Libadwaita..."
  podman run --rm --userns=keep-id \
    -v mithrandir-cargo-cache:/var/tmp/cargo:Z -e CARGO_HOME=/var/tmp/cargo \
    -v "${ROOT_DIR}:/workspace:Z" -w /workspace localhost/dev-fedora \
    cargo build --release --features gui
fi

BUILD_DIR="${ROOT_DIR}/target/pkg-flatpak"
REPO_DIR="${ROOT_DIR}/target/repo-flatpak"
rm -rf "${BUILD_DIR}" "${REPO_DIR}"
mkdir -p "${BUILD_DIR}" "${REPO_DIR}"

echo "==> Initializing Flatpak environment with native flatpak tool..."
flatpak build-init "${BUILD_DIR}" "${APP_ID}" org.gnome.Platform org.gnome.Platform 50

echo "==> Staging application files into /app..."
mkdir -p "${BUILD_DIR}/files"/{bin,share/applications,share/glib-2.0/schemas,share/metainfo,share/icons/hicolor}
install -m 755 "${ROOT_DIR}/target/release/mithrandir" "${BUILD_DIR}/files/bin/mithrandir"
install -m 644 "${ROOT_DIR}/data/io.github.antonio.Mithrandir.desktop" "${BUILD_DIR}/files/share/applications/${APP_ID}.desktop"
install -m 644 "${ROOT_DIR}/data/io.github.antonio.Mithrandir.gschema.xml" "${BUILD_DIR}/files/share/glib-2.0/schemas/${APP_ID}.gschema.xml"
install -m 644 "${ROOT_DIR}/data/io.github.antonio.Mithrandir.metainfo.xml" "${BUILD_DIR}/files/share/metainfo/${APP_ID}.metainfo.xml"
cp -r "${ROOT_DIR}/data/icons/hicolor/"* "${BUILD_DIR}/files/share/icons/hicolor/"

glib-compile-schemas "${BUILD_DIR}/files/share/glib-2.0/schemas"

echo "==> Finalizing Flatpak permissions and metadata..."
flatpak build-finish "${BUILD_DIR}" \
  --command=mithrandir \
  --share=network \
  --share=ipc \
  --socket=fallback-x11 \
  --socket=wayland \
  --device=dri \
  --filesystem=home \
  --filesystem=~/.var/app \
  --filesystem=xdg-cache \
  --filesystem=xdg-config/autostart \
  --talk-name=org.freedesktop.Flatpak \
  --system-talk-name=org.freedesktop.systemd1 \
  --talk-name=org.freedesktop.secrets \
  --talk-name=org.kde.StatusNotifierWatcher \
  --system-talk-name=org.scx.Loader \
  --system-talk-name=org.freedesktop.UPower.PowerProfiles \
  --system-talk-name=net.hadess.PowerProfiles

echo "==> Exporting to OSTree repository..."
flatpak build-export "${REPO_DIR}" "${BUILD_DIR}" master

echo "==> Exporting Flatpak Single-File Bundle..."
flatpak build-bundle "${REPO_DIR}" "${BUNDLE_PATH}" "${APP_ID}" master

echo "==> Flatpak bundle generated successfully at:"
echo "    ${BUNDLE_PATH}"
