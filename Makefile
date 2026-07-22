PREFIX ?= /usr
BINDIR ?= $(PREFIX)/bin
LIBEXECDIR ?= $(PREFIX)/libexec
DATADIR ?= $(PREFIX)/share
SYSTEMDUNITDIR ?= $(PREFIX)/lib/systemd/system

.PHONY: all build build-gui check test test-lib fmt fmt-check clippy lint \
	install install-cli install-post gui-smoke clean

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

lint: fmt-check clippy

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
