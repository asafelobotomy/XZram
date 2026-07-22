# XZram

Cross-distro Linux swap management: zram, swap files, partitions, and sysctl tuning.

XZram is a CLI-first tool for creating, removing, and customizing swap on systemd-based
Linux distributions. It includes a Qt6 GUI and system D-Bus daemon (`xzramd`).

## Features

- **Read-only diagnostics** — `status`, `detect`, and `doctor` without root
- **Staged apply** — write commands stage to `/var/lib/xzram/pending.json`; `apply` executes atomically
- **ZRAM management** — configure via `systemd-zram-generator`; migrate from legacy zram-tools
- **Swap file management** — create, resize, remove disk-backed swap (btrfs nodatacow enforced)
- **Swap partitions** — `swap list` merges active swaps and fstab partitions
- **Sysctl tuning** — swappiness, watermark, and page-cluster settings
- **Polkit integration** — granular privileged operations (no blanket sudo)
- **D-Bus daemon** — `xzramd` on `io.github.XZram1`; auto-starts on first GUI/CLI `--dbus` use via D-Bus activation
- **Qt6 GUI** — dashboard, zram, swapfile, sysctl, doctor, and utilities (snapshot restore)
- **Configuration snapshots** — automatic backups on GUI open and before every apply; versioned restore

## Supported distros

| Family | Detection / runtime | Packaging in this repo |
|--------|---------------------|------------------------|
| Fedora / RHEL / CentOS Stream | Full | [`packaging/xzram.spec`](packaging/xzram.spec) |
| Debian / Ubuntu / derivatives | Full | [`debian/`](debian/) |
| Arch / CachyOS / Manjaro | Full | [`PKGBUILD`](PKGBUILD) |
| openSUSE | Full | Packaging TBD (detection supported) |
| Gentoo | Partial | — |
| NixOS / Alpine / non-systemd | Out of scope v1 | — |

Published distro packages are not assumed; use From source or the in-tree packaging files
when building locally.

## Prerequisites

- **CLI / daemon:** Rust ≥ 1.75, Cargo, Linux with systemd
- **Full install / GUI:** cmake, Qt6 Widgets + DBus
  - Arch: `qt6-base`, `cmake`
  - Debian/Ubuntu: `qt6-base-dev`, `cmake`, `pkg-config`
  - Fedora: `qt6-qtbase-devel`, `cmake`

## Quick start

```bash
# Build from source (release; slow cold build — fine for packaging)
cargo build --release

# Faster iteration while developing
cargo run -p xzram-cli -- status
make check
make test-lib

# Read-only commands (no root required)
./target/release/xzram status
./target/release/xzram detect
./target/release/xzram doctor

# Stage changes (default) then apply (apply needs polkit / root via helper)
./target/release/xzram zram set --size "min(ram / 2, 4096)"
./target/release/xzram apply

# Immediate apply for scripting
./target/release/xzram zram set --now
```

Do **not** use `sudo` for smoke checks of `status` / `detect` / `doctor`.

## Verify

```bash
make lint          # fmt --check + clippy -D warnings (matches CI)
make test-lib      # fast unit tests for crates/xzram
make test          # full cargo test workspace
make gui-smoke     # Qt6 offscreen launch smoke (needs GUI deps)
```

Diagnostics: `xzram status` and `xzram doctor` are the operator health surface (no root).
For binary tracing: `RUST_LOG=debug cargo run -p xzram-cli -- status` (see [docs/DEV-ENV.md](docs/DEV-ENV.md)).

Optional local git hooks (same gates as CI lint + lib tests):

```bash
pip install pre-commit   # or distro package: pre-commit
pre-commit install
```

See [AGENTS.md](AGENTS.md) for agent-oriented bootstrap notes and
[docs/DEV-ENV.md](docs/DEV-ENV.md) for test overrides (`XZRAM_*`).

## Installation

### From source (recommended for checkouts)

```bash
# CLI + daemon + polkit/D-Bus (no Qt)
make install-cli DESTDIR=/tmp/xzram-install
sudo cp -a /tmp/xzram-install/* /

# Or full install including xzram-qt (requires cmake + Qt6)
make install DESTDIR=/tmp/xzram-install
sudo cp -a /tmp/xzram-install/* /

# Privileged next step: enable the system daemon
sudo systemctl enable --now xzramd
```

Prefer `DESTDIR` staging over installing as root into the source tree so
`build-gui/` does not become root-owned. For reinstall/uninstall helpers see
[`scripts/reinstall-system.sh`](scripts/reinstall-system.sh) and
[`scripts/uninstall-system.sh`](scripts/uninstall-system.sh).

### Build GUI only

```bash
make build-gui
# Binary: build-gui/xzram-qt/xzram-qt
```

### Arch / CachyOS (packaging source)

The in-tree [`PKGBUILD`](PKGBUILD) is the packaging source. For a normal developer
checkout, prefer **From source** above. `makepkg -si` expects a packaging-oriented
layout; it is not the primary path for iterating on a git clone.

### Debian

```bash
dpkg-buildpackage -us -uc -b
sudo dpkg -i ../xzram_*.deb
sudo systemctl enable --now xzramd
```

### Fedora

Build from [`packaging/xzram.spec`](packaging/xzram.spec) with `rpmbuild` (or your
usual RPM workflow). Enable `xzramd` after install as on other distros.

## CLI reference

```
xzram status              # all swap devices, zram stats, priorities
xzram doctor              # detect zswap conflict, hibernation issues
xzram detect              # distro, backend, installed packages

xzram zram show|set|disable|migrate
xzram swapfile list|create|resize|remove
xzram swap list|on|off    # list includes fstab partitions
xzram sysctl show|set
xzram defaults recommend|stage|apply
xzram pending show|clear  # inspect/clear staged config
xzram daemon start        # enable and start xzramd (pkexec)
xzram apply               # apply staged configuration
xzram snapshot list       # list configuration snapshots
xzram snapshot restore last-apply
xzram rollback            # alias: restore last pre-apply snapshot

Global flags: --json, --dbus, --now (on write commands)
```

See [docs/SNAPSHOTS.md](docs/SNAPSHOTS.md) for snapshot semantics and retention.
See [docs/RECOMMENDATIONS.md](docs/RECOMMENDATIONS.md) for recommended defaults.

## Architecture

```
xzram CLI / xzram-qt GUI
        │
        ├── read-only ──► xzram lib
        ├── pkexec ──► xzram-helper ──► xzram lib
        └── --dbus ──► xzramd (D-Bus) ──► xzram lib
```

See [docs/SCOPE.md](docs/SCOPE.md) for project scope and
[docs/GUI-PHASE2.md](docs/GUI-PHASE2.md) for the Qt6 GUI and D-Bus daemon architecture.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
