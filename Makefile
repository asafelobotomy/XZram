PREFIX ?= /usr
BINDIR ?= $(PREFIX)/bin
LIBEXECDIR ?= $(PREFIX)/libexec
DATADIR ?= $(PREFIX)/share

.PHONY: all build test install clean

all: build

build:
	cargo build --release

test:
	cargo test

install: build
	install -Dm755 target/release/xzram $(DESTDIR)$(BINDIR)/xzram
	install -Dm755 target/release/xzram-helper $(DESTDIR)$(LIBEXECDIR)/xzram-helper
	install -Dm644 data/io.github.xzram.policy $(DESTDIR)$(DATADIR)/polkit-1/actions/io.github.xzram.policy
	install -Dm644 data/bash-completion/xzram $(DESTDIR)$(DATADIR)/bash-completion/completions/xzram

clean:
	cargo clean
