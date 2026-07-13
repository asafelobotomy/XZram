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

| Family | Support |
|--------|---------|
| Fedora / RHEL / CentOS Stream | Full |
| Debian / Ubuntu / derivatives | Full |
| Arch / CachyOS / Manjaro | Full |
| openSUSE | Full |
| Gentoo | Partial |
| NixOS / Alpine / non-systemd | Out of scope v1 |

## Quick start

```bash
# Build from source
cargo build --release

# Read-only commands (no root required)
./target/release/xzram status
./target/release/xzram detect
./target/release/xzram doctor

# Stage changes (default) then apply
./target/release/xzram zram set --size "min(ram / 2, 4096)"
./target/release/xzram apply

# Immediate apply for scripting
./target/release/xzram zram set --now
```

## Installation

### Arch / CachyOS

```bash
makepkg -si
sudo systemctl enable --now xzramd
```

### Debian

```bash
dpkg-buildpackage -us -uc -b
sudo dpkg -i ../xzram_*.deb
sudo systemctl enable --now xzramd
```

### From source

```bash
make install DESTDIR=/tmp/xzram-install
sudo cp -a /tmp/xzram-install/* /
sudo systemctl enable --now xzramd
```

## CLI reference

```
xzram status              # all swap devices, zram stats, priorities
xzram doctor              # detect zswap conflict, hibernation issues
xzram detect              # distro, backend, installed packages

xzram zram show|set|disable|migrate
xzram swapfile list|create|resize|remove
xzram swap list|on|off    # list includes fstab partitions
xzram sysctl show|set
xzram pending show|clear  # inspect/clear staged config
xzram daemon start        # enable and start xzramd (pkexec)
xzram apply               # apply staged configuration
xzram snapshot list       # list configuration snapshots
xzram snapshot restore last-apply
xzram rollback            # alias: restore last pre-apply snapshot

Global flags: --json, --dbus, --now (on write commands)
```

See [docs/SNAPSHOTS.md](docs/SNAPSHOTS.md) for snapshot semantics and retention.

## Architecture

```
xzram CLI / xzram-qt GUI
        │
        ├── read-only ──► xzram lib
        ├── pkexec ──► xzram-helper ──► xzram lib
        └── --dbus ──► xzramd (D-Bus) ──► xzram lib
```

See [docs/SCOPE.md](docs/SCOPE.md) for project scope and [docs/GUI-PHASE2.md](docs/GUI-PHASE2.md)
for the Qt6 GUI and D-Bus daemon design.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
