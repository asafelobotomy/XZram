# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Security
- Path/INI/argv hardening for swapfile, zram-generator, and swap activate (validation + `--` separators)
- Advisory store lock for pending/snapshot mutations; helper validates staged pending
- Artifact modes: `/var/lib/xzram` `0o700`, pending/index/last_error `0o600`
- Best-effort `restorecon -F` after swapfile create/resize when SELinux tools are present
- GUI CLI error JSON built with `QJsonDocument` (no manual quote escaping)
- SteamOS/immutable detection expanded (`steamos` id/VARIANT, `steamos-readonly`, `/etc/steamos-release`)
- pkexec helper path restricted to annotated `/usr/libexec/xzram-helper` (dev override requires `XZRAM_ALLOW_DEV_HELPER`)
- xzramd `systemd-run` uses `--expand-environment=no` and `RuntimeMaxSec=300`
- zram-tools finalize only runs when pending was staged via migrate (not on unrelated applies)
- Swap device validation rejects `..` path components

### Added
- Hermetic unit tests for snapshot restore, recommend engine (`recommend_from_context`), and sysctl-only apply under `XZRAM_ETC_ROOT`
- Packaging hints for zram backends: PKGBUILD `optdepends`, debian `Recommends: systemd-zram-generator`, RPM `Recommends: zram-generator`

### Changed
- `XZRAM_ETC_ROOT` honored for zram-generator conf and sysctl drop-in writes; restore skips privileged steps under hermetic etc roots
- Living audit register rewrite ([docs/AUDIT.md](docs/AUDIT.md)); Snapshot GUI docs updated for delete/prune
- Doctor marks hibernation-on-zram as Error (not Warning)
- Snapshot prune rejects `keep=0`; CLI validates swap devices before pkexec
- Sysctl drop-in merge-writes so partial `--now` does not drop sibling keys
- Migrate maps zram-tools `SIZE`/`PRIORITY`; recommend size evaluator understands percent formulas
- CI: `permissions: contents: read`, `--locked` builds, Linux-only matrix; Debian package build from RO mount + copy
- Packaging: PKGBUILD source is repo root (not parent); RPM `%preun`/`%license`; Debian systemd `--no-enable --no-start`
- Upstream URLs point at `https://github.com/asafelobotomy/XZram`

### Removed
- Flatpak packaging path (`flatpak/` manifest and `docs/FLATPAK.md`); distribution is native packages only (PKGBUILD, debian/, RPM)

### Fixed
- Desktop/AppStream theme icon: install `hicolor` sizes 32–512 as `io.github.XZram.png` in Makefile, PKGBUILD, Debian, RPM; set Qt `desktopFileName` and `StartupWMClass` for taskbar matching
- Debian `prerm` no longer disables xzramd on upgrade; `postinst` defers enable/start to dh_installsystemd
- Migrate finalize propagates `systemctl disable --now` failures
- Swap partition listing soft-fails unresolved UUIDs; active match uses canonicalize
- Overflow recommend skips when free-space probe fails; bash-completion covers `snapshot`
- `xzramd` unit declares `StateDirectory=xzram`; Makefile `install-post` refuses `DESTDIR`

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
