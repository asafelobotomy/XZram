# XZram Cross-Distro Safety Audit

**Date:** 2026-07-13  
**Scope:** CLI (`xzram`), privileged helper (`xzram-helper`), D-Bus daemon (`xzramd`), Qt GUI (`xzram-qt`), packaging (PKGBUILD, RPM spec), polkit, and shared library (`crates/xzram`).

**Goal:** Verify every user-facing action is robust, clean, and safe on major systemd-based Linux distributions (Arch/CachyOS, Debian/Ubuntu, Fedora/RHEL, openSUSE).

---

## Executive summary

XZram’s architecture — **stage → apply** with backups, polkit-gated privileged operations, and backend abstraction over `systemd-zram-generator` + swapfiles — is sound for cross-distro use. Detection covers Fedora, Debian, Ubuntu, Arch, openSUSE, and Gentoo families.

This audit found **five critical (P0) issues**, several **high (P1)** gaps, and **medium (P2)** polish items. **Most P0 and several P1 items were fixed during this audit** (see §3). Remaining work is documented in §4.

**Overall posture:** Safe for production use on systemd distros **after** installing `xzramd` + polkit policy and ensuring `zram-generator` is present. GUI users without `xzramd` now get correct pkexec paths for recommended-defaults apply.

---

## Methodology

1. **Surface inventory** — mapped every CLI subcommand, D-Bus method, helper action, and GUI button to its code path.
2. **Privilege model** — traced pkexec/polkit authorization for each mutating operation.
3. **Filesystem safety** — reviewed path validation, backup/rollback, fstab editing, btrfs handling.
4. **Distro matrix** — checked detection, package names, init assumptions, and packaging artifacts.
5. **Automated tests** — `cargo test` (40+ unit tests in `crates/xzram`, plus CLI/helper smoke) plus manual review of integration gaps.

---

## Architecture & trust boundaries

```
┌─────────────┐     read-only      ┌──────────────┐
│  xzram CLI  │ ─────────────────► │  xzram lib   │
│  xzram-qt   │                    │  (detect,    │
└──────┬──────┘                    │   doctor,    │
       │ stage/apply               │   recommend) │
       ▼                           └──────┬───────┘
┌─────────────┐   polkit check    ┌──────▼───────┐
│   xzramd    │ ◄──────────────── │ xzram-helper │
│  (system    │   or pkexec       │  (root)      │
│   bus)      │ ─────────────────►└──────────────┘
└─────────────┘         apply
       │
       ▼
 /etc/systemd/zram-generator.conf
 /etc/fstab, /etc/sysctl.d/99-xzram.conf
 /var/lib/xzram/{pending.json,backup/}
```

| Layer | Runs as | Mutates system? | Auth |
|-------|---------|-----------------|------|
| CLI read (`status`, `detect`, `doctor`, `defaults recommend`) | user | No | — |
| CLI write (`set`, `apply`, `swapfile *`) | root via pkexec helper | Yes | polkit actions |
| xzramd read APIs | user (bus policy) | No | — |
| xzramd write APIs | user caller → polkit in daemon | Yes | `zbus_polkit` per method |
| xzram-helper | root (pkexec) | Yes | polkit exec annotations |
| GUI | user | Via dbus/helper | Same as above |

---

## Findings by severity

### P0 — Critical (fixed unless noted)

| ID | Issue | Impact | Status |
|----|-------|--------|--------|
| P0-1 | `xzramd` polkit subject used bus `peer_creds` (daemon UID) instead of message caller | Unprivileged callers were authorized as root; polkit checks ineffective | **Fixed (2026-07)** — `Subject::new_for_message_header(header)` in `crates/xzramd/src/dbus/auth.rs`; verify unprivileged `xzram --dbus apply` prompts or denies |
| P0-2 | Missing `io.github.xzram.zram.migrate` polkit action | Migrate callable without matching policy | **Fixed** — added to `data/io.github.xzram.policy` |
| P0-3 | Btrfs swapfile apply failed without prior `prepare` | Recommended overflow swapfile broken on btrfs roots | **Fixed** — auto `prepare_nodatacow` in `swapfile.rs` before create |
| P0-4 | `disable_zram` D-Bus applied immediately | Bypassed stage/review model; surprising side effects | **Fixed** — stages via `PendingConfig`; CLI `--dbus zram.disable` now stages then `Apply` |
| P0-5 | Vendor-only zram config not fully disabled | `disable()` did not match upstream empty-override semantics | **Fixed** — writes empty `/etc/systemd/zram-generator.conf` |

### P1 — High

| ID | Issue | Impact | Status |
|----|-------|--------|--------|
| P1-1 | No rollback manifest for first-time apply | First apply creates new files; rollback cannot remove swapfile or delete new zram config | **Fixed** — versioned snapshot subsystem with manifest, swapfile cleanup, and recreation ([docs/SNAPSHOTS.md](SNAPSHOTS.md)) |
| P1-2 | zram-tools migration incomplete | Legacy `zramswap.service` may conflict after migrate | **Fixed** — `finalize_zram_tools_migration()` on apply |
| P1-3 | GUI `defaults apply` via CLI fallback without pkexec | Normal users could not apply recommendations when `xzramd` absent | **Fixed** — unified stage + `applyPending` pkexec path in `mainwindow.cpp` / `dbusclient.cpp` |
| P1-4 | JSON error string not escaped in D-Bus client | Malformed JSON / injection in error display if message contains quotes | **Fixed** — `QJsonDocument` encoding in `dbusclient.cpp` |
| P1-5 | `find_helper()` preferred `~/.local` over `/usr/libexec` | Dev installs shadowed system helper unexpectedly | **Fixed** — system paths first; `XZRAM_DEV_HELPER` override |
| P1-6 | Swapfile path validation missing | Paths under `/boot`, relative paths, `..` traversal | **Fixed** — `crates/xzram/src/validation.rs` |
| P1-7 | fstab editing fragile | Partial line matches, no backup on resize | **Fixed** — exact matching, backup on resize, `swapoff` errors propagated |
| P1-8 | zram configure wiped multi-device configs | `zram1+` lost on single-device edit | **Fixed** — merge by device name in `zram_generator.rs` |
| P1-9 | `xzram-qt` not packaged | Desktop entry points to missing binary on Arch/RPM installs | **Fixed** — bundled in PKGBUILD, RPM spec, and debian/ |
| P1-10 | D-Bus bus policy allows all users to *call* methods | Surface area for probing; mitigated by in-daemon polkit | **Accepted risk** — polkit enforces auth; tightening bus policy is optional hardening |

### P2 — Medium / polish

| ID | Issue | Status |
|----|-------|--------|
| P2-1 | `xzramd.service` lacks systemd hardening (`ProtectSystem`, `PrivateTmp`, etc.) | **Fixed** |
| P2-2 | No immutable-OS detection (Silverblue, NixOS, SteamOS) | **Fixed** — `etc_writable` / `immutable_os` in detect; doctor warning |
| P2-3 | Hibernation check may false-positive on swapfile-only setups | **Fixed** — resume device resolved to block name; warn only when resume is zram |
| P2-4 | GUI `findHelperBinary()` still falls back to `~/.local` after system paths | **OK** — intentional for dev; system paths checked first |
| P2-5 | No debian/`.deb` packaging directory | **Fixed** — `debian/` with rules, postinst, prerm |
| P2-6 | Flatpak / Phase 2 sandbox story undocumented for production | **Fixed** — [docs/FLATPAK.md](FLATPAK.md) |

---

## Per-surface matrix

### CLI (`crates/xzram-cli`)

| Command | Privilege | Validation | Backup | Notes |
|---------|-----------|------------|--------|-------|
| `status`, `detect`, `doctor` | none | N/A | N/A | Safe read-only |
| `zram set`, `zram disable`, `swapfile *`, `sysctl set` | pkexec helper | partial → improved | on apply | Staged in `pending.json` |
| `apply` | pkexec helper | pending schema | yes | Applies staged bundle |
| `rollback` | pkexec helper | backup dir exists | restores | Cannot undo first-time file creation (P1-1) |
| `defaults recommend` | none | N/A | N/A | Hardware-aware profiles |
| `defaults stage` / `apply` | pkexec | recommend output | on apply | Overflow swapfile auto-staged |
| `swapfile check/prepare` | none / pkexec | path validation | N/A | Btrfs nodatacow |
| `zram migrate` | pkexec | zramswap exists | on apply | Does not stop legacy service (P1-2) |

### Helper (`crates/xzram-helper`)

- Single JSON payload interface; all actions logged via `tracing`.
- Unknown actions return validation error (no silent fallback).
- `apply` with `{}` applies pending — matches CLI/GUI contract.

### Daemon (`crates/xzramd`)

| Method | Auth action | Read/Write |
|--------|-------------|------------|
| `GetStatus`, `GetDetection`, `RunDoctor`, `GetZramConfig`, `ListSwapfiles`, `ListSwaps`, `GetSysctl`, `GetPending`, `GetRecommendedDefaults`, `ListSnapshots`, `GetSnapshot` | none (read) | Read |
| `CheckSwapfileBtrfs` | none | Read |
| `StageAction`, `StageRecommendedDefaults`, `ConfigureZram`, `DisableZram`, `CreateSwapfile`, `RemoveSwapfile`, `ResizeSwapfile`, `SetSysctl`, `Apply`, `Rollback`, `ClearPending`, `MigrateZram` | matching `io.github.xzram.*` | Write |
| `PrepareSwapfileBtrfs` | `io.github.xzram.swapfile.prepare` (via privileged helper) | Write |
| `CreateSnapshot` | `io.github.xzram.snapshot.create` | Write |
| `RestoreSnapshot` / `DeleteSnapshot` / `PruneSnapshots` | `snapshot.restore` / `snapshot.delete` | Write |

**Follow-up (2026-07 remediation):** CreateSnapshot gated; prepare routed through helper; StageAction validates `swapfile_resize`; Rollback uses `io.github.xzram.rollback`; helper runs under `spawn_blocking` with 300s timeout; pending/snapshot mutates serialized with a mutex; unit `Restart=on-failure`.

### GUI (`gui/xzram-qt`)

| Feature | Path | Status |
|---------|------|--------|
| Dashboard / status | CLI (`xzram --json`) | OK |
| Zram / swapfile / sysctl tabs | CLI stage + apply pending | OK |
| Doctor | CLI read-only | OK |
| Recommended defaults dialog | CLI stage + apply (pkexec) | **Fixed** (P1-3) |
| Btrfs prepare | CLI / helper | OK |
| Pending banner | CLI pending | OK |
| Native GUI ↔ xzramd | not used | intentional — daemon for `--dbus` / Flatpak |

### Packaging

| Artifact | Ships CLI | Ships helper | Ships daemon | Ships GUI | Ships polkit |
|----------|-----------|--------------|--------------|-----------|--------------|
| `PKGBUILD` | yes | yes | yes | **no** | yes |
| `packaging/xzram.spec` | yes | yes | yes | **no** | yes |
| `.desktop` file | — | — | — | references `xzram-qt` | — |

**Recommendation:** Either add `xzram-qt` to packages (split `xzram-gui` subpackage) or gate the desktop entry behind a GUI package until Phase 2 ships.

---

## Distro-specific notes

### Arch / CachyOS / Manjaro

- **Backend:** `zram-generator` via `systemd-zram-generator` package.
- **Detection:** `DistroFamily::Arch`; CachyOS triggers performance recommendation profile.
- **Btrfs:** Common root FS — auto nodatacow prepare on apply is essential (P0-3).
- **Packaging:** PKGBUILD ready; enable `xzramd.service` on install.

### Debian / Ubuntu / Mint / Pop

- **Backend:** `systemd-zram-generator` (Debian 11+, Ubuntu 22.04+).
- **Package name:** `systemd-zram-generator` (doctor suggests via `detect::zram_generator_package_name`).
- **polkit:** `auth_admin` defaults appropriate for server/desktop.
- **Gap:** No `.deb` maintainer scripts; RPM `%post` enables systemd unit but Debian equivalent missing.

### Fedora / RHEL / Rocky / Alma

- **Backend:** `zram-generator` RPM subpackage of systemd or standalone depending on release.
- **SELinux:** Swapfile creation uses `fallocate` + `mkswap`; may need `swapfile_t` context on enforcing systems — **not yet audited automatically** (manual verify on Fedora recommended).
- **Packaging:** `xzram.spec` includes `%systemd_post` hooks — good.

### openSUSE

- **Detection:** `DistroFamily::OpenSuse`; `zypper` package manager mapping present.
- **zram-generator:** Available on Leap/Tumbleweed; same code paths as Fedora.

### Gentoo

- Detected but less tested; relies on user-installed `systemd-zram-generator`.

### Immutable / special

| OS | Risk | Current behavior |
|----|------|------------------|
| Fedora Silverblue / Kinoite | `/etc` writes fail or are ephemeral | No specific warning (P2-2) |
| NixOS | No `/etc/fstab` model | Unsupported — doctor `non_systemd` path insufficient |
| Containers/WSL | No zram or polkit | Backend detection returns unavailable — acceptable |

---

## Strengths

1. **Staged apply model** — users review `pending.json` before destructive changes.
2. **Backup before mutate** — `create_backup()` snapshots existing zram/fstab/sysctl before edits.
3. **Shared library** — CLI, daemon, helper, and tests use one implementation.
4. **Doctor + recommendations alignment** — `checks.rs` shared between doctor and `recommend.rs`.
5. **Distro detection** — `os-release` parsing with family classification and package hints.
6. **Path validation** — swapfile paths blocked under `/boot`, `/dev`, etc.
7. **Btrfs awareness** — check, prepare, and auto-prepare on create.
8. **Polkit granularity** — separate actions per operation (configure, disable, swapfile, migrate, …).

---

## Recommended next steps

### Before 1.0

1. **P1-1:** Add rollback manifest (`created_files`, `created_units`) written at apply time; teach `rollback()` to remove swapfiles and delete xzram drop-ins when backup proves they did not exist before.
2. **P1-2:** On `zram migrate` apply: `systemctl disable --now zramswap.service`, move `/etc/default/zramswap` → `.bak`.
3. **P1-9:** Ship `xzram-qt` subpackage or remove desktop entry from main package.
4. **P2-1:** Harden `xzramd.service` (see systemd.exec man page).
5. **P2-2:** Doctor check for read-only `/etc` or `OSTREE=1`.

### Testing matrix (manual)

| Scenario | Arch | Debian | Fedora |
|----------|------|--------|--------|
| Fresh install + `defaults apply` | ☐ | ☐ | ☐ |
| Btrfs overflow swapfile | ☐ | ☐ | ☐ |
| zram-tools migrate | ☐ | ☐ | ☐ |
| GUI without xzramd (pkexec only) | ☐ | ☐ | ☐ |
| polkit deny → graceful error | ☐ | ☐ | ☐ |
| `rollback` after config change | ☐ | ☐ | ☐ |

---

## Files touched in this audit

| Area | Files |
|------|-------|
| Polkit / D-Bus auth | `crates/xzramd/src/dbus/auth.rs`, `data/io.github.xzram.policy` |
| Zram disable / merge | `crates/xzram/src/backend/zram_generator.rs` |
| Swapfile safety | `crates/xzram/src/validation.rs`, `crates/xzram/src/backend/swapfile.rs` |
| Helper discovery | `crates/xzram-cli/src/main.rs` |
| GUI pkexec / JSON | `gui/xzram-qt/dbusclient.cpp`, `gui/xzram-qt/mainwindow.cpp` |

---

## Related documentation

- [SCOPE.md](SCOPE.md) — supported backends and out-of-scope items
- [RECOMMENDATIONS.md](RECOMMENDATIONS.md) — hardware/distro recommendation matrix
- [GUI-PHASE2.md](GUI-PHASE2.md) — GUI/daemon milestone status
