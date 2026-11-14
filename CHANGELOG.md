# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- `xzram defaults apply --at-boot` / `--force`: the memory guard's refusal named these flags, but `defaults apply` didn't accept them
- D-Bus `ApplyZramDisableWithOptions(b at_boot, b force) → as`, so D-Bus clients can get past the memory guard when disabling zram (the CLI no longer falls back to pkexec for this)
- GUI: when Apply or Apply recommended defaults is refused for lack of free memory, offer **Apply at next boot** or **Restart anyway**; success dialogs show the CLI's messages (the at-boot note, the force warning)
- `make install-cli-files` / `make install-files`: copy an existing build without building, so a source install builds as the user and runs only the copy as root

### Changed
- Doctor `swap_nearly_full` is an error when swap is ≥ 98% used and less than 15% of RAM is available (previously a warning at any level, so a system with exhausted swap reported "Healthy")
- `xzram-helper` refuses unknown fields in every action's payload (stage, zram.migrate, snapshot.*, swapfile.*, swap.activate), not only some
- A release `xzram-helper` running as root ignores `XZRAM_*` development overrides (except `XZRAM_ERROR_FILE`)

### Fixed
- `xzram swapfile prepare`, xzramd `PrepareSwapfileBtrfs` and the GUI's Doctor "prepare" button failed with "unknown action: swapfile.prepare": the helper action had been dropped
- uQ page: a swappiness typed into the box couldn't be applied until Enter was pressed; Apply now uses the typed number
- README source install: the `DESTDIR` + `sudo cp -a` recipe left `/usr/libexec/xzram-helper` (run as root by pkexec) and `/usr` owned by the building user. Use `make build && sudo make install-cli-files`
- AUR release workflow: the build job, which runs crate build scripts, no longer holds a write token; uploading to the release is a separate job with no checkout
- `snapshot create` over pkexec now checks the helper version like every other pkexec action
- zram-generator.conf is written atomically (temp file, fsync, rename). A crash or full disk mid-write could leave it empty, which disables zram at the next boot
- Changing zram keeps the rest of zram-generator.conf: other `[zramN]` sections are no longer dropped when the current file can't be read (the change is refused instead), and keys xzram doesn't model (`writeback-device`, `options`, `host-memory-limit`, ...) are kept on every rewrite. A value xzram doesn't accept elsewhere in the file (e.g. `zstd(level=3)`) no longer blocks every change
- `defaults recommend` never replaces a zram size it can't evaluate (zram-generator accepts `^`, `%` and functions xzram doesn't model), so it can't shrink a larger custom device
- The zram-generator config is looked up in systemd's order (`/etc` before `/run`)
- A zram device counts as swap only when it is in /proc/swaps. An initialised zram ramdisk (e.g. `/tmp`) was counted as swap, so the memory guard could refuse a restart for it. A zram device that disappears mid-scan no longer fails `status`, `doctor` and `recommend`

## [0.6.0] — 2026-11-13

### Added
- `xzram apply --at-boot` / `--force`, and the same flags on `zram set` / `zram disable`: restarting zram (which swaps its stored data back into RAM) is now memory-checked first. Below `MemAvailable` ≥ stored data (`mm_stat` orig_data_size) + max(512 MiB, 5% of RAM) it is refused with "not enough free memory" and nothing changes; `--at-boot` writes the generator config only (takes effect at the next boot), `--force` restarts anyway. D-Bus: `ApplyWithOptions(b at_boot, b force)` and `ApplyZramWithOptions(s config, b at_boot, b force)`; `ApplyNowZram` and `Apply` are memory-checked too (#28)
- `xzram swap drain-overflow [--dry-run] [--force]`: opt-in swapoff + swapon (same priority) of disk swap below zram's priority, so overflow moves back into a grown zram; same memory guard (zram free capacity counted at the measured ratio). New polkit action `io.github.xzram.swap.drain` (helper `swap.drain-overflow`, pkexec only) (#28)
- `xzram status` shows memory and I/O PSI (`pressure` in `--json`) (#13)
- uQ page: when a zram change is refused for lack of memory, **Apply at next restart** saves it for the next boot (`ApplyZramWithOptions` with `at_boot = true`, never forced)
- Doctor `swapfile_blocks_snapshots` (error): an active swapfile in the btrfs root subvolume, `/home`, or a snapper-configured subvolume makes every snapshot of it fail ("cannot snapshot subvolume with active swapfile"); recommend lists it as a host fix with the `@swap` subvolume steps

### Changed
- Doctor headline reflects warnings: "System healthy", "Healthy, 1 warning" or "Issues found: 1 error, 2 warnings". The JSON report keeps `healthy` (errors only, exit code 3) and adds `errors`, `warnings` and `summary`; the GUI dashboard chip and Doctor tab use them (#28)
- Doctor `zram_near_full` only counts disk swap below zram's priority as overflow, and becomes an error when the overflow is significant (≥ max(256 MiB, 10% of the device)) and memory PSI shows stalls (full avg60 ≥ 5%, some avg60 ≥ 20% or some avg300 ≥ 10%) (#28)
- `defaults recommend` grows a saturated zram (≥ 90% full with disk overflow) instead of reporting "already matches": a ratio-based `min(ram * m, 65536)` sized so a full device costs at most ~50% of RAM at the measured compression ratio, or the High tier without a ratio; never smaller than the current device (#13, #28)
- Conservative zram defaults: 12–31 GiB RAM (MemTotal ≥ 11 GiB) now `min(ram, 16384)` (was `min(ram / 2, 4096)`), ≥ 32 GiB `min(ram / 2, 16384)` (was `min(ram / 2, 8192)`); Low/High tiers adjusted to match (see docs/RECOMMENDATIONS.md). 4–11 GiB, constrained and performance profiles are unchanged (#13, #28)
- Size formulas are evaluated with a small expression parser (`+ - * /`, parentheses, `min`/`max`, decimals), so sizes like `ram * 1.5` are compared correctly when deciding whether to grow

### Fixed
- Creating a swapfile (directly or via `defaults apply`) inside a snapshotted btrfs subvolume is refused with the steps to move swap to a dedicated `@swap` subvolume; recommend no longer stages or grows the overflow swapfile there. Previously `/swap/swapfile` could be created in a plain directory inside `@`, after which snapper timelines and snap-pac snapshots failed with ETXTBSY
- Doctor `btrfs_swapfile_nodatacow` (and recommend's matching prepare step) no longer fires for an active swapfile whose parent lacks +C, e.g. one made by `btrfs filesystem mkswapfile` in a fresh `@swap` subvolume: btrfs only activates NOCOW swapfiles, and XZram sets +C on the parent itself before recreating one

## [0.5.0] — 2026-11-12

### Added
- A "Memory and swap" page for [uQ](https://github.com/asafelobotomy/uq) Settings (`uq/pages/xzram/`): memory and swap in use, zram size and compression, swappiness and the swap list. It talks to `xzramd` over D-Bus (`ApplyNowZram`, `ApplyNowSysctl`), sends the whole zram configuration back with one field changed, and asks before changing zram. Swap files stay in the XZram app. Installed by `make install-cli` and both PKGBUILDs (`uq` is an optional dependency); CI gains a `uq` job (Node and QML tests, install check, `uqctl integrations check`)

### Changed
- Dependencies: zbus_polkit 5.0.0 → 5.1.0 (RUSTSEC-2026-0278; `xzramd` uses `new_for_message_header`, not the affected `new_for_owner`, so it was not exploitable) and tokio 1.52.3 → 1.54.1 (RUSTSEC-2026-0386); async-executor moves off a yanked release
- CI: a `cargo deny check advisories` job (`deny.toml` ignores only the unmaintained zbus 5 family). Project files: contributing guide, code of conduct, security policy, CODEOWNERS, issue and PR templates, Dependabot

### Fixed
- Snapshot restore no longer panics inside the privileged restore when the snapshot path is not valid UTF-8 (it called `to_str().unwrap()`)

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

[Unreleased]: https://github.com/asafelobotomy/XZram/compare/v0.5.0...HEAD
[0.5.0]: https://github.com/asafelobotomy/XZram/compare/v0.4.3...v0.5.0
[0.4.3]: https://github.com/asafelobotomy/XZram/compare/v0.4.2...v0.4.3
[0.4.2]: https://github.com/asafelobotomy/XZram/compare/v0.4.1...v0.4.2
[0.4.1]: https://github.com/asafelobotomy/XZram/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/asafelobotomy/XZram/compare/v0.3.2...v0.4.0
[0.3.2]: https://github.com/asafelobotomy/XZram/compare/v0.3.0...v0.3.2
[0.3.0]: https://github.com/asafelobotomy/XZram/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/asafelobotomy/XZram/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/asafelobotomy/XZram/releases/tag/v0.1.0
