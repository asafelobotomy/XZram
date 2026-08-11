# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.0] — 2026-08-11

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
- D-Bus `GetPending` / `ListSnapshots` / `GetSnapshot` require polkit `io.github.xzram.store.read`
- GUI `XZRAM_CLI` override gated behind `XZRAM_ALLOW_DEV_CLI`; prefers `/usr/bin/xzram`
- CI GitHub Actions pinned to full commit SHAs
- Sysctl values validated to kernel-aligned ranges (swappiness 0–200, watermarks 0–10000, page-cluster 0–8)

### Added
- Hermetic unit tests for snapshot restore, recommend engine (`recommend_from_context`), and sysctl-only apply under `XZRAM_ETC_ROOT`
- Packaging hints for zram backends: PKGBUILD `optdepends`, debian `Recommends: systemd-zram-generator`, RPM `Recommends: zram-generator`
- `xzram snapshot create --trigger app_open|manual`; GUI startup best-effort AppOpen snapshot
- Split packages: `xzram` (CLI/daemon) and `xzram-gui` (Qt)
- Linked auto-optimize: `xzram defaults optimize-linked` plus GUI Settings toggle (default on) that live-rewrites related ZRAM/sysctl/swapfile fields to recommend-aligned values
- Recommend size scales (`--zram-scale` / `--swap-scale`) and Low / Recommended / High sliders in the defaults dialog

### Changed
- `XZRAM_ETC_ROOT` honored for zram-generator conf and sysctl drop-in writes; restore skips privileged steps under hermetic etc roots
- Living audit register rewrite ([docs/AUDIT.md](docs/AUDIT.md)); Snapshot GUI docs updated for delete/prune
- Doctor marks hibernation-on-zram as Error (not Warning)
- Snapshot prune rejects `keep=0`; CLI validates swap devices before pkexec
- Sysctl drop-in merge-writes so partial `--now` does not drop sibling keys
- Migrate maps zram-tools `SIZE`/`PRIORITY`; recommend size evaluator understands percent formulas
- CI: `permissions: contents: read`, `--locked` builds, Linux-only matrix; Debian package build from RO mount + copy
- Packaging: PKGBUILD source is repo root (not parent); RPM `%preun`/`%license`; Debian systemd `--no-enable --no-start`
- Debian Depends: `polkitd | policykit-1 | polkit`; GUI relies on `${shlibs:Depends}` (no nonexistent `qt6-base`); postinst no longer embeds `#DEBHELPER#` in a comment
- Upstream URLs point at `https://github.com/asafelobotomy/XZram`
- Snapshot `state_hash` includes canonical runtime swapfile and zram metadata
- CLI `pending show` / `snapshot list` fall back to D-Bus when store is not readable

### Removed
- Flatpak packaging path (`flatpak/` manifest and `docs/FLATPAK.md`); distribution is native packages only (PKGBUILD, debian/, RPM)

### Fixed
- Desktop/AppStream theme icon: install `hicolor` sizes 32–512 as `io.github.XZram.png` in Makefile, PKGBUILD, Debian, RPM; set Qt `desktopFileName` and `StartupWMClass` for taskbar matching
- Linked optimize: keep ZRAM Stage enabled after paint (no baseline reset); run optimize-linked via async `CliJob` with stdin instead of blocking the UI thread
- Debian `prerm` no longer disables xzramd on upgrade; `postinst` defers enable/start to dh_installsystemd
- Migrate finalize propagates `systemctl disable --now` failures
- Swap partition listing soft-fails unresolved UUIDs; active match uses canonicalize
- Overflow recommend skips when free-space probe fails; bash-completion covers `snapshot`
- `xzramd` unit declares `StateDirectory=xzram`; Makefile `install-post` refuses `DESTDIR`
- GUI Configure recommended defaults no longer wipes staged tab preview via `refreshAll`
- Partition swap Enable/Disable require confirmation dialogs
- GUI CLI errors prefer this-process stderr/stdout over shared `/var/lib/xzram/last_error`
- Doctor issue cards HTML-escape CLI messages; JSON uint64 parsing avoids raw double cast
- GUI long CLI ops (apply, defaults apply, snapshot restore/rollback, swapfile create/resize, AppOpen) run via async `CliJob` with cancelable progress dialog; startup AppOpen deferred off the constructor
- Accept `,` / `^` / `%` in zram-size expressions so recommended `min(ram / 2, 4096)` formulas pass helper validation

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

[0.3.0]: https://github.com/asafelobotomy/XZram/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/asafelobotomy/XZram/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/asafelobotomy/XZram/releases/tag/v0.1.0
