.PHONY: all build test check clean appimage flatpak deb aur publish

all: build flatpak deb appimage aur

build:
	podman run --rm --userns=keep-id \
	  -v mithrandir-cargo-cache:/var/tmp/cargo:Z -e CARGO_HOME=/var/tmp/cargo \
	  -v "$$(pwd):/workspace:Z" -w /workspace localhost/dev-fedora \
	  cargo build --release --features gui

check:
	podman run --rm --userns=keep-id \
	  -v mithrandir-cargo-cache:/var/tmp/cargo:Z -e CARGO_HOME=/var/tmp/cargo \
	  -v "$$(pwd):/workspace:Z" -w /workspace localhost/dev-fedora \
	  cargo check --features gui

test:
	podman run --rm --userns=keep-id \
	  -v mithrandir-cargo-cache:/var/tmp/cargo:Z -e CARGO_HOME=/var/tmp/cargo \
	  -v "$$(pwd):/workspace:Z" -w /workspace localhost/dev-fedora \
	  cargo test

appimage:
	./packaging/build_appimage.sh

flatpak:
	./packaging/build_flatpak.sh

deb:
	./packaging/build_deb.sh

aur:
	@echo "==> AUR PKGBUILD is ready in packaging/aur/PKGBUILD"

publish:
	./packaging/publish_github.sh

clean:
	cargo clean
	rm -rf target/pkg-flatpak target/repo-flatpak target/pkg-appimage target/pkg-deb *.AppImage *.flatpak *.deb
