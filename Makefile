PREFIX ?= /usr
BINDIR ?= $(PREFIX)/bin
LIBEXECDIR ?= $(PREFIX)/libexec
DATADIR ?= $(PREFIX)/share
SYSTEMDUNITDIR ?= $(PREFIX)/lib/systemd/system

.PHONY: all build test install install-post clean

all: build

build:
	cargo build --release

test:
	cargo test

install: build
	install -Dm755 target/release/xzram $(DESTDIR)$(BINDIR)/xzram
	install -Dm755 target/release/xzram-helper $(DESTDIR)$(LIBEXECDIR)/xzram-helper
	install -Dm755 target/release/xzramd $(DESTDIR)$(LIBEXECDIR)/xzramd
	install -Dm644 data/io.github.xzram.policy $(DESTDIR)$(DATADIR)/polkit-1/actions/io.github.xzram.policy
	install -Dm644 data/bash-completion/xzram $(DESTDIR)$(DATADIR)/bash-completion/completions/xzram
	install -Dm644 data/io.github.XZram.service $(DESTDIR)$(SYSTEMDUNITDIR)/xzramd.service
	install -Dm644 data/io.github.XZram.conf $(DESTDIR)$(DATADIR)/dbus-1/system.d/io.github.XZram.conf
	install -Dm644 data/dbus-1/system-services/io.github.XZram1.service $(DESTDIR)$(DATADIR)/dbus-1/system-services/io.github.XZram1.service
	install -Dm644 data/io.github.XZram.desktop $(DESTDIR)$(DATADIR)/applications/io.github.XZram.desktop
	install -Dm644 data/io.github.XZram.metainfo.xml $(DESTDIR)$(DATADIR)/metainfo/io.github.XZram.metainfo.xml

install-post:
	systemctl daemon-reload
	systemctl enable xzramd.service

clean:
	cargo clean
