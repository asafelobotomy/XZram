# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed
- Desktop/AppStream theme icon: install `hicolor` sizes 32–512 as `io.github.XZram.png` in Makefile, PKGBUILD, Debian, RPM, Flatpak; set Qt `desktopFileName` and `StartupWMClass` for taskbar matching

## [0.2.0] — 2026-07-22

### Added
- Qt GUI CLI-first runner (`XzramCli`); daemon optional for other clients
- Settings tab (auto-refresh, confirm-before-apply, prune keep, CLI/daemon status)
- Snapshot tab (create, restore, delete, prune, rollback)
- App icon and desktop icon install path
- Recommended-defaults hardening: overflow cap, fstab/free-space gates, immutable/RO hard stops, vendor zram-size respect
- Concise button tooltips across the GUI

### Changed
- Apply recommended defaults dialog copy (Apply now vs stage for review)
- Pending banner labels (Apply now / Discard)
- Doctor/sysctl/swap UX wording for clearer actions

### Removed
- GUI D-Bus client path (`dbusclient`, `clifallback`) and Qt6 DBus dependency
- Utilities tab (split into Snapshot + Settings)

## [0.1.0] — 2026-07-10

### Added
- Initial CLI, helper, daemon, polkit, and Qt6 GUI
- Staged apply, zram/swapfile/sysctl management, doctor, snapshots
- Packaging stubs (Arch PKGBUILD, Fedora spec, Debian)

[0.2.0]: https://github.com/xzram/xzram/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/xzram/xzram/releases/tag/v0.1.0
