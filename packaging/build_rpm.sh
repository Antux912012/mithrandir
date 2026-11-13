#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

VERSION=$(grep -m1 '^version' "${ROOT_DIR}/Cargo.toml" | sed -E 's/.*"([^"]+)".*/\1/')
APP_ID="io.github.antux912012.Mithrandir"
RPM_NAME="mithrandir-${VERSION}-1.x86_64.rpm"

echo "==> Building Mithrandir RPM package (${RPM_NAME})..."

if [[ ! -f "${ROOT_DIR}/target/release/mithrandir" ]]; then
  echo "==> Release binary not found, compiling first..."
  podman run --rm --userns=keep-id \
    -v mithrandir-cargo-cache:/var/tmp/cargo:Z -e CARGO_HOME=/var/tmp/cargo \
    -v "${ROOT_DIR}:/workspace:Z" -w /workspace localhost/dev-fedora \
    cargo build --release --features gui
fi

RPM_TOP="${ROOT_DIR}/target/pkg-rpm"
rm -rf "${RPM_TOP}"
mkdir -p "${RPM_TOP}"/{BUILD,RPMS,SOURCES,SPECS,SRPMS}

cat > "${RPM_TOP}/SPECS/mithrandir.spec" << EOF
%global debug_package %{nil}
Name:           mithrandir
Version:        ${VERSION}
Release:        1%{?dist}
Summary:        Native GTK4 & Libadwaita Linux System Optimizer, Cleaner & CachyOS Power Manager
License:        GPL-3.0-or-later
URL:            https://github.com/Antux912012/mithrandir
Requires:       gtk4 libadwaita glib2 polkit hicolor-icon-theme

%description
Blazing-fast native GNU/Linux system optimizer, multi-threaded deep cache
cleaner, CachyOS BORE scheduler controller, hardware inspector, and AI
optimization assistant written in Rust with GTK4 and Libadwaita.

%install
mkdir -p %{buildroot}%{_bindir}
mkdir -p %{buildroot}%{_datadir}/applications
mkdir -p %{buildroot}%{_datadir}/glib-2.0/schemas
mkdir -p %{buildroot}%{_datadir}/metainfo
mkdir -p %{buildroot}%{_datadir}/polkit-1/actions
mkdir -p %{buildroot}%{_datadir}/icons/hicolor
mkdir -p %{buildroot}%{_udevrulesdir}

install -m 755 /workspace/target/release/mithrandir %{buildroot}%{_bindir}/mithrandir
ln -sf /usr/bin/mithrandir %{buildroot}%{_bindir}/mithrandir-rs
install -m 644 /workspace/data/${APP_ID}.desktop %{buildroot}%{_datadir}/applications/${APP_ID}.desktop
install -m 644 /workspace/data/${APP_ID}.gschema.xml %{buildroot}%{_datadir}/glib-2.0/schemas/${APP_ID}.gschema.xml
install -m 644 /workspace/data/${APP_ID}.metainfo.xml %{buildroot}%{_datadir}/metainfo/${APP_ID}.metainfo.xml
install -m 644 /workspace/data/${APP_ID}.policy %{buildroot}%{_datadir}/polkit-1/actions/${APP_ID}.policy
install -m 644 /workspace/data/99-mithrandir-power.rules %{buildroot}%{_udevrulesdir}/99-mithrandir-power.rules
cp -r /workspace/data/icons/hicolor/* %{buildroot}%{_datadir}/icons/hicolor/

%post
/usr/bin/glib-compile-schemas %{_datadir}/glib-2.0/schemas >/dev/null 2>&1 || :
/usr/bin/gtk-update-icon-cache -f -t %{_datadir}/icons/hicolor >/dev/null 2>&1 || :

%files
%{_bindir}/mithrandir
%{_bindir}/mithrandir-rs
%{_datadir}/applications/${APP_ID}.desktop
%{_datadir}/glib-2.0/schemas/${APP_ID}.gschema.xml
%{_datadir}/metainfo/${APP_ID}.metainfo.xml
%{_datadir}/polkit-1/actions/${APP_ID}.policy
%{_udevrulesdir}/99-mithrandir-power.rules
%{_datadir}/icons/hicolor/*
EOF

podman run --rm --userns=keep-id \
  -v "${ROOT_DIR}:/workspace:Z" -w /workspace localhost/dev-fedora \
  rpmbuild -bb --define "_topdir /workspace/target/pkg-rpm" --target x86_64 "/workspace/target/pkg-rpm/SPECS/mithrandir.spec"

BUILT_RPM=$(find "${RPM_TOP}/RPMS" -name "mithrandir-*.rpm" | head -n1)
cp "${BUILT_RPM}" "${ROOT_DIR}/${RPM_NAME}"

echo "==> RPM package generated successfully at:"
echo "    ${ROOT_DIR}/${RPM_NAME}"
