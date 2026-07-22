PREFIX ?= /usr
BINDIR ?= $(PREFIX)/bin
LIBEXECDIR ?= $(PREFIX)/libexec
DATADIR ?= $(PREFIX)/share
SYSTEMDUNITDIR ?= $(PREFIX)/lib/systemd/system

.PHONY: all build build-gui check test test-lib fmt fmt-check clippy lint loc-check \
	install install-cli install-post gui-smoke clean version-check

XZRAM_VERSION := $(shell tr -d '[:space:]' < VERSION)

all: build

build:
	cargo build --release

build-gui:
	cmake -S gui -B build-gui
	cmake --build build-gui

check:
	cargo check -p xzram

test:
	cargo test

test-lib:
	cargo test -p xzram --lib

fmt:
	cargo fmt --all

fmt-check:
	cargo fmt --all -- --check

clippy:
	cargo clippy --all-targets -- -D warnings

lint: fmt-check clippy loc-check

loc-check:
	./scripts/check-loc.sh

# CLI/daemon/polkit/dbus only (no Qt).
install-cli: build
	install -Dm755 target/release/xzram $(DESTDIR)$(BINDIR)/xzram
	install -Dm755 target/release/xzram-helper $(DESTDIR)$(LIBEXECDIR)/xzram-helper
	install -Dm755 target/release/xzramd $(DESTDIR)$(LIBEXECDIR)/xzramd
	install -Dm644 data/io.github.xzram.policy $(DESTDIR)$(DATADIR)/polkit-1/actions/io.github.xzram.policy
	install -Dm644 data/bash-completion/xzram $(DESTDIR)$(DATADIR)/bash-completion/completions/xzram
	install -Dm644 data/io.github.XZram.service $(DESTDIR)$(SYSTEMDUNITDIR)/xzramd.service
	install -Dm644 data/io.github.XZram.conf $(DESTDIR)$(DATADIR)/dbus-1/system.d/io.github.XZram.conf
	install -Dm644 data/dbus-1/system-services/io.github.XZram1.service $(DESTDIR)$(DATADIR)/dbus-1/system-services/io.github.XZram1.service

# Full install including Qt GUI and desktop metadata.
install: build build-gui install-cli
	install -Dm755 build-gui/xzram-qt/xzram-qt $(DESTDIR)$(BINDIR)/xzram-qt
	install -Dm644 data/io.github.XZram.desktop $(DESTDIR)$(DATADIR)/applications/io.github.XZram.desktop
	install -Dm644 data/io.github.XZram.metainfo.xml $(DESTDIR)$(DATADIR)/metainfo/io.github.XZram.metainfo.xml
	install -Dm644 branding/xzram-icon.png \
		$(DESTDIR)$(DATADIR)/icons/hicolor/256x256/apps/io.github.XZram.png

install-post:
	systemctl daemon-reload
	systemctl enable --now xzramd.service

# Headless GUI launch smoke (timeout = success; binary must start under offscreen QPA).
gui-smoke: build-gui
	test -x build-gui/xzram-qt/xzram-qt
	@set +e; \
	QT_QPA_PLATFORM=offscreen timeout 3 build-gui/xzram-qt/xzram-qt; \
	code=$$?; \
	if [ $$code -eq 124 ] || [ $$code -eq 0 ]; then exit 0; else exit $$code; fi

clean:
	cargo clean
	rm -rf build-gui

# Ensure VERSION matches Cargo workspace, CMake project, and packaging metadata.
version-check:
	@test -n "$(XZRAM_VERSION)"
	@grep -q 'version = "$(XZRAM_VERSION)"' Cargo.toml
	@grep -q 'project(xzram-qt VERSION $(XZRAM_VERSION)' gui/CMakeLists.txt
	@grep -q '^pkgver=$(XZRAM_VERSION)$$' PKGBUILD
	@grep -q '^Version:[[:space:]]*$(XZRAM_VERSION)$$' packaging/xzram.spec
	@grep -q 'version="$(XZRAM_VERSION)"' data/io.github.XZram.metainfo.xml
	@head -1 debian/changelog | grep -q "$(XZRAM_VERSION)-"
	@echo "version $(XZRAM_VERSION) OK"