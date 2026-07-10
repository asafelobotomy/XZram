# XZram

Cross-distro Linux swap management: zram, swap files, partitions, and sysctl tuning.

XZram is a CLI-first tool for creating, removing, and customizing swap on systemd-based
Linux distributions. A Qt6 GUI is planned for a later phase.

## Features

- **Read-only diagnostics** — `status`, `detect`, and `doctor` without root
- **ZRAM management** — configure via `systemd-zram-generator`
- **Swap file management** — create, resize, remove disk-backed swap
- **Sysctl tuning** — swappiness, watermark, and page-cluster settings
- **Polkit integration** — granular privileged operations (no blanket sudo)

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

# Privileged operations use pkexec + polkit
./target/release/xzram zram show
./target/release/xzram apply
```

## Installation

### Arch / CachyOS

```bash
makepkg -si
```

### From source

```bash
cargo install --path crates/xzram-cli
sudo install -Dm644 data/io.github.xzram.policy /usr/share/polkit-1/actions/
```

## CLI reference

```
xzram status              # all swap devices, zram stats, priorities
xzram doctor              # detect zswap conflict, hibernation issues
xzram detect              # distro, backend, installed packages

xzram zram show|set|disable
xzram swapfile list|create|resize|remove
xzram swap list|on|off
xzram sysctl show|set
xzram apply               # validate config + apply changes
xzram rollback            # restore last known-good config snapshot
```

## Architecture

```
xzram CLI  ──►  xzram lib (core logic)
                    │
                    ├── read-only: /proc/swaps, zramctl, sysfs
                    └── privileged: pkexec xzram-helper (polkit)
```

See [docs/SCOPE.md](docs/SCOPE.md) for project scope and [docs/GUI-PHASE2.md](docs/GUI-PHASE2.md)
for the planned Qt6 GUI and D-Bus daemon.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
