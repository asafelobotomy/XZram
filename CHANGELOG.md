# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.4.3] — 2026-10-14

### Fixed
- Doctor `oom_kills` no longer repeats "grow swap, enable an OOM killer" once protection is active: past kills are still listed, as info, with advice on reading what was killed (MGLRU `min_ttl_ms` deliberately OOM-kills instead of thrashing). Recommend drops its "OOM killer fired recently" note in that case
- GUI: dashboard swap table sizes Type / Status / Used / Priority / Source to their contents (the path column takes the slack), so "Used / size" is no longer elided; the size cell has a tooltip
- CI: `btrfs_root_tip_skipped_when_swapfiles_configured` holds the test env lock; it raced tests that repoint `XZRAM_ETC_ROOT` and failed intermittently (0.3.2 pin, 0.4.1, 0.4.2 runs)

## [0.4.2] — 2026-10-14

### Fixed
- A CLI newer than the installed helper no longer half-applies: helpers before 0.4.2 silently dropped payload fields they did not know (0.4.1's oomd, memory tunables and new sysctls were ignored while apply reported success). The helper and `xzramd` now parse privileged payloads strictly and name any unsupported field; the CLI refuses helpers older than 0.4.2 (checked via the new `xzram-helper --version`) before prompting
- Helper `stage` / `apply` pick the payload shape by its `pending` / `prepare_swapfile` keys instead of retrying a failed parse as the other shape (which turned real errors into "missing field" noise)
- Root `PKGBUILD` (local checkout package) builds again: the `file://$startdir` source made makepkg save a directory listing instead of the tree; it now uses `git+file://` (committed HEAD) and depends on `git`

## [0.4.1] — 2026-10-14

### Fixed
- systemd-oomd policy no longer risks killing the whole desktop session: apply also writes `/etc/systemd/user/session.slice.d/10-xzram-oomd.conf` (`ManagedOOMPreference=avoid`) so apps in `app.slice` die before the compositor and shell, and reloads running user managers. Hosts already running oomd with a `user@` pressure kill policy get just this guard staged (Ubuntu 26.10 dropped that policy after it ended sessions)

### Added
- Kernel memory tunables (`mm` in pending, persisted via `/etc/tmpfiles.d/xzram-mm.conf`, live sysfs write, snapshot restore writes old values back): MGLRU `min_ttl_ms=1000` thrashing prevention and THP `defrag=defer+madvise`
- Sysctl: `vm.dirty_bytes` (256 MiB), `vm.dirty_background_bytes` (64 MiB) and `vm.vfs_cache_pressure` (50) are staged with recommended defaults; `xzram sysctl set` gains `--dirty-bytes`, `--dirty-background-bytes`, `--vfs-cache-pressure`
- Doctor: `oomd_session_unprotected`, `thp_always_low_ram`
- GUI: pending banner counts memory tunables; "Memory" recommendation chip

## [0.4.0] — 2026-10-14

### Fixed
- Recommended defaults grow an undersized `/swap/swapfile` (e.g. a distro-installed 512 MiB file) to the overflow target instead of reporting "already matches"; previously any active disk swap suppressed the overflow safety net

### Added
- Recommended defaults warn when disk swap is (or would be) placed on a rotational drive
- OOM protection: when no userspace OOM killer can act, recommended defaults stage `enable_oomd`; apply installs `ManagedOOMSwap=kill` (`-.slice`) and `ManagedOOMMemoryPressure=kill` (`user@.service`) drop-ins and enables systemd-oomd. Snapshots track the drop-ins and service state, so rollback undoes it
- Doctor checks: `no_oom_protection`, `oomd_no_policies`, `oom_kills` (this and previous boot), `memory_pressure_high` / `memory_pressure_elevated` (PSI), `swap_nearly_full`, `disk_overflow_small` / `no_disk_overflow`, `swap_on_rotational`
- GUI: pending banner counts `enable_oomd`; recommendation chips label zswap and OOM-protection items

## [0.3.2] — 2026-10-11

### Security
- Swapfile create/resize/remove/prepare require every existing ancestor directory to be root-owned and not group/other-writable (when running as root), reject symlinked ancestors and non-regular leaves, and allocate/chmod via `O_NOFOLLOW` file descriptors (closes a symlink race against root writes)
- Release builds ignore `XZRAM_DEV_HELPER` (CLI) and the GUI no longer honours `XZRAM_CLI` or searches `PATH` unless built with `-DXZRAM_DEV_OVERRIDES=ON`
- xzramd: read-only methods run on the blocking pool behind a 4-slot semaphore (no unauthenticated worker starvation); `ApplyNow*` forward re-serialized validated payloads; sysctl values validated in the daemon; `MigrateZram` validates staged pending; `PrepareSwapfileBtrfs` takes the mutation gate
- xzramd unit hardened (ProtectKernelLogs/Clock/Hostname, LockPersonality, RestrictSUIDSGID/Realtime, `IPAddressDeny=any`, `AF_UNIX` only, capability deny-list); `PrivateDevices` replaced by `DevicePolicy=closed` + read-only block devices
- Callers can no longer create `pre_apply` snapshots directly

### Fixed
- `/etc/fstab`, `pending.json`, snapshot index/manifests, sysctl drop-in, zram override and `last_error` are written atomically (temp + fsync + rename)
- fstab swap entries are matched by exact decoded path (not substring); re-creating a swapfile updates its `pri=`; `\NNN` escapes decoded; backslashes rejected in new swap paths
- Swapfile resize builds the new file beside the old one and only swaps it in once ready; failures leave the old swap active. When there isn't room for both, it resizes in place (needs only the growth delta) and restores the original size on failure; if even the delta doesn't fit, nothing is touched
- Privileged helper runs are serialized by a process-held lock (`/var/lib/xzram/helper.lock`), so pkexec and xzramd helpers never run concurrently, including across xzramd restarts
- Swapfile create refuses to replace an active swap (use resize)
- Snapshot restore merges only fstab swap entries and removes swapfiles created after the snapshot (previously a no-op)
- Failed `apply` automatically restores the pre-apply snapshot and keeps pending for retry
- Snapshot list works for non-root users (snapshots root `0711`, index `0644`; permission errors surface instead of returning empty)
- Snapshot dates use a correct calendar conversion; `zram disable` attempts all devices and always writes the override
- xzramd no longer times out a running helper at 300 s nor releases its gate while the helper runs; per-call error files prevent misattributed errors
- `--dbus` falls back to pkexec only when the service is unavailable before the call (no double execution on `NoReply`); `snapshot prune --keep` bounded and no longer truncates over D-Bus
- Cancelled/denied polkit prompts report that instead of a stale `last_error`
- GUI: privileged operations run asynchronously with a busy dialog and no kill-on-timeout; live refresh is non-blocking; doctor results are shown when the system is unhealthy

### Changed
- CLI exit codes: 0 ok, 1 error, 2 usage, 3 `doctor` unhealthy (was 1)
- Helper direct actions take a `pre_apply` snapshot first; polkit messages for `stage`/`apply`/`swapfile.create`/`zram.migrate` describe their full effect
- D-Bus policy allows `Introspectable`, `Peer` and `Properties.Get/GetAll` for unprivileged callers
- `make version-check` moved to `scripts/version-check.sh`: also checks AUR `.SRCINFO` pkgver, PKGBUILD/`.SRCINFO` `pkgver` and `sha256sums` agreement, rejects `SKIP`, rejects reusing an earlier version's pinned checksum after a bump, checks the RPM `%changelog` top entry, and warns on the placeholder sum; now run in CI
- Packaging (AUR): `.SRCINFO` was stale at 0.3.0 while the PKGBUILD said 0.3.1; now regenerated; the stale 0.3.0 checksum in `packaging/aur/PKGBUILD` is replaced by an explicit all-zero placeholder until the release tag is published and release CI pins the real sum
- Packaging (Arch): optdepend renamed `systemd-zram-generator` → `zram-generator` (the Arch package name) in both PKGBUILDs and `.SRCINFO`
- Packaging (Arch): `xzram.install` no longer runs `systemctl enable` on install/upgrade; it only prints a hint (xzramd is D-Bus activated, enabling is optional) and is now wired via `install=` in both PKGBUILDs (copy shipped in `packaging/aur/`)
- Packaging (RPM): `Source0` points at the GitHub tag archive and `%autosetup -n XZram-%{version}` matches its top-level directory; `%changelog` de-duplicated (Aug 11 entry is 0.3.0-1, 0.1.0 dated Jul 10); `%doc` ships `SCOPE.md`/`SNAPSHOTS.md` (and `GUI-PHASE2.md` for the GUI) like the PKGBUILD
- Packaging (Debian): `debhelper-compat (= 13)` replaces `debian/compat`; docs installed via `debian/*.docs`


## [0.3.1] — 2026-09-23

### Changed
- Privilege UX: `defaults apply` and `zram migrate --now` use one polkit challenge; swapfile create `--prepare` folds Btrfs prepare into the same stage/create call
- D-Bus stage-only methods (`ConfigureZram`, etc.) authorize `io.github.xzram.stage` instead of mutate action IDs
- Store metadata: data dir `0755`, `pending.json` and snapshot index `0644` (payload archives stay `0700`/`0600`); `store.read` is `allow_active=yes`
- GUI no longer auto-creates an AppOpen snapshot on startup (Apply PreApply remains the automatic baseline)
- Empty `xzram apply` and empty `StageRecommendedDefaults` skip privilege

### Removed
- Dead helper verb `snapshot.list`
- GUI startup AppOpen `pkexec` path

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
- `status`/`doctor`/GUI no longer mislabel zram's `mm_stat` `mem_used_max` field as a compression-stream count (`streams`); reported as `peak <size>` instead — the concept doesn't exist on kernels without `max_comp_streams`
- Doctor warns (`zram_near_full`) when a zram swap device is ≥90% full and disk-backed swap is already absorbing overflow
- `xzramd` registers its D-Bus interface before requesting the bus name, removing a startup race (and its zbus warning) where early method calls could be lost
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

[Unreleased]: https://github.com/asafelobotomy/XZram/compare/v0.4.3...HEAD
[0.4.3]: https://github.com/asafelobotomy/XZram/compare/v0.4.2...v0.4.3
[0.4.2]: https://github.com/asafelobotomy/XZram/compare/v0.4.1...v0.4.2
[0.4.1]: https://github.com/asafelobotomy/XZram/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/asafelobotomy/XZram/compare/v0.3.2...v0.4.0
[0.3.2]: https://github.com/asafelobotomy/XZram/compare/v0.3.0...v0.3.2
[0.3.0]: https://github.com/asafelobotomy/XZram/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/asafelobotomy/XZram/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/asafelobotomy/XZram/releases/tag/v0.1.0
