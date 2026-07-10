# XZram Project Scope

## Goal

Provide a unified CLI (and later Qt6 GUI) for full swap management on major
systemd-based Linux distributions.

## In scope (v1)

- ZRAM configuration via `systemd-zram-generator`
- Swap file create / resize / remove with fstab persistence
- Swap partition listing and activation control
- Sysctl tuning (swappiness, watermark_boost_factor, watermark_scale_factor, page-cluster)
- Read-only diagnostics: status, detect, doctor
- Polkit-gated privileged operations
- Native packaging: PKGBUILD, debian/, rpm spec
- CI testing on Fedora, Ubuntu, Debian, Arch

## Out of scope (v1)

- NixOS declarative configuration
- Alpine / non-systemd init systems
- Immutable distro full support (Silverblue overlay UX deferred)
- Hibernation setup (detect and warn only)
- GUI (phase 2)

## Backend abstraction

| Backend | Config surface |
|---------|----------------|
| systemd-zram-generator | `/etc/systemd/zram-generator.conf` |
| Swap file | `/etc/fstab`, `mkswap`, `swapon` |
| Swap partition | `/etc/fstab`, `blkid` |
| zram-tools (legacy) | `/etc/default/zramswap` |

## Edge cases

- **zram vs zswap**: doctor warns when zswap is enabled
- **Hibernation**: doctor warns if resume device is zram
- **Btrfs swapfiles**: require nodatacow; doctor checks filesystem type
- **Priority tiers**: zram high (100), disk swap low (10) by default

## Permissions

All write operations require polkit `auth_admin`. Read-only commands work
without elevation.
