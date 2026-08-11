# XZram Cross-Distro Safety Audit

**Updated:** 2026-08-11  
**Scope:** CLI (`xzram`), privileged helper (`xzram-helper`), D-Bus daemon (`xzramd`), Qt6 GUI (`xzram-qt`), packaging (PKGBUILD, debian/, RPM), polkit, and shared library (`crates/xzram`).

Living risk register for systemd-based Linux distros (Arch/CachyOS, Debian/Ubuntu, Fedora/RHEL, openSUSE). Historical July 2026 P0/P1 remediation notes are summarized below; prefer this file over chat/canvas for release gating.

---

## Executive summary

Architecture (**stage → apply**, caller-bound polkit, hardened `xzramd` + root helper) is sound for mutable systemd distros. Flatpak packaging was **removed** (native packages only). Input validation, store locking, hermetic tests, artifact modes, and packaging Recommends for zram-generator landed in the 2026-08 audit follow-up.

**Overall posture:** Safe for production on systemd distros after installing packages (GUI optional) and ensuring a zram backend (`zram-generator` / `systemd-zram-generator` via Recommends/optdepends).

---

## Trust boundaries

| Layer | Runs as | Mutates system? | Auth |
|-------|---------|-----------------|------|
| CLI read (`status`, `detect`, `doctor`, `defaults recommend`) | user | No | — |
| CLI write (`set`, `apply`, `swapfile *`) | root via pkexec helper | Yes | polkit actions |
| xzramd read APIs | user (bus policy) | No | — |
| xzramd write APIs | user caller → polkit in daemon | Yes | `zbus_polkit` per method |
| xzram-helper | root (pkexec) | Yes | polkit exec annotations |
| GUI | user | Via CLI/pkexec | Same as CLI |

---

## Mitigated findings (S-01..S-19, Q-01..Q-04)

| ID | Topic | Status |
|----|-------|--------|
| S-01 | Fstab line injection via path newlines | **Mitigated** — path validation rejects controls/newlines |
| S-02 | Remove arbitrary absolute paths | **Mitigated** — remove only if fstab swap entry or active file swap; expanded deny prefixes |
| S-03 | `swap.activate` argv flag injection | **Mitigated** — `swapon`/`swapoff` use `--` separator; device validation |
| S-04 | `swapfile.prepare` skips path validation | **Mitigated** — helper validates prepare path |
| S-05 | Symlink before allocate/remove | **Mitigated** — `lstat` rejects symlink path components; leaf must be a regular file |
| S-06 | INI / unit-name injection in zram-generator.conf | **Mitigated** — device/expression/token validation |
| S-07 | Flatpak security model mismatch | **Mitigated** — Flatpak path removed; native packages only |
| S-08 | `pending.json` RMW races | **Mitigated** — advisory `store.lock` flock |
| S-09 | Rollback / ApplyNow* skip mutation gate | **Mitigated** — daemon mutex gate on rollback and ApplyNow* |
| S-10 | `--now` paths apply entire pending | **Mitigated** — ApplyNow* call helper directly (no stage+Apply of full pending) |
| S-11 | `snapshot.prune` missing argv1 annotation | **Mitigated** — polkit policy annotated |
| S-12 | Unit hardening vs privileged helper escape | **Partial** — xzramd hardened; helper transient unit is full-priv by design |
| S-13 | SELinux labeling after swapfile create | **Mitigated** — best-effort `restorecon -F` when present |
| S-14 | Helper stage skips field validation | **Mitigated** — `validate_staged_pending` in helper |
| S-15 | `/var/lib/xzram` artifact modes | **Mitigated** — data dir `0755`, pending/index `0644` (readable metadata); snapshot payloads/`last_error` stay private; `store.read` `allow_active=yes` |
| S-16 | GUI JSON error escaping incomplete | **Mitigated** — `QJsonDocument`/`QJsonObject` in `xzramcli.cpp` |
| S-17 | Immutable-OS coverage (SteamOS) | **Mitigated** — steamos id/VARIANT + `steamos-readonly` / `steamos-release` |
| S-18 | Bus policy allows all users to call Manager | **Partial** — writes polkit-gated; `GetPending` / snapshot reads require `io.github.xzram.store.read`; status/detect/doctor remain public |
| S-19 | D-Bus mutators authorize caller | **Mitigated** — `Subject::new_for_message_header` (historical P0-1) |
| Q-01 | `snapshot/restore.rs` untested | **Mitigated** — hermetic restore tests under `XZRAM_ETC_ROOT` |
| Q-02 | `recommend/engine.rs` untested | **Mitigated** — `recommend_from_context` + unit tests |
| Q-03 | Stale `docs/AUDIT.md` | **Mitigated** — this living register |
| Q-04 | No Recommends for zram-generator | **Mitigated** — PKGBUILD optdepends; debian Recommends; RPM Recommends |

### Historical P0 (July 2026)

Caller-bound polkit, migrate action, btrfs auto-prepare, staged disable, empty-override disable — remain fixed.

---

## Packaging

| Artifact | CLI package | GUI package | zram-generator hint |
|----------|-------------|-------------|---------------------|
| `PKGBUILD` | `xzram` | `xzram-gui` | `optdepends` on CLI |
| `debian/` | `xzram` | `xzram-gui` | `Recommends` on CLI |
| `packaging/xzram.spec` | `xzram` | `xzram-gui` | `Recommends` on CLI |

Flatpak: **removed** (see [SCOPE.md](SCOPE.md)). Cargo vendor for offline distro builds remains deferred.

---

## Third-pass findings (2026-08-11)

Surfaces newly reviewed: GUI/CLI argv, packaging/CI/install, polkit/D-Bus policy, migrate/sysctl/swap listing, recommend overflow.

### Privilege / migrate (batch A)

| ID | Topic | Status |
|----|-------|--------|
| T3-01 | pkexec of non-annotated helper / `XZRAM_DEV_HELPER` | **Mitigated** |
| T3-02 | `systemd-run` env expansion of helper argv | **Mitigated** |
| T3-03 | Helper timeout without unit RuntimeMaxSec | **Mitigated** |
| T3-M01 | Finalize zram-tools on any apply | **Mitigated** |
| T3-M02 | Migrate finalize ignores systemctl failure | **Mitigated** |
| T3-D01 | Hibernate-on-zram shown healthy | **Mitigated** |
| T3-H02 | `prune --keep 0` deletes all snapshots | **Mitigated** |
| T3-deb | Debian prerm disable-on-upgrade / invalid start | **Mitigated** |

### Correctness / packaging (batch B)

| ID | Topic | Status |
|----|-------|--------|
| SYSCTL-01 | Partial sysctl `--now` overwrites drop-in | **Mitigated** |
| MIG-01 | Migrate ignores `SIZE`/`PRIORITY` | **Mitigated** |
| MIG-02 | `eval_zram_size_mb` misses percent formulas | **Mitigated** |
| SWAP-01/02 | UUID list / active match | **Mitigated** |
| REC-01 | Overflow when `df` fails | **Mitigated** |
| PKG-01..09 | PKGBUILD/RPM/Debian/CI packaging | **Mitigated** (vendor deferred) |
| INST-01..04 | PREFIX docs, install-post, StateDirectory, completion | **Mitigated** / documented |

### Residuals closure (batch C)

| ID | Topic | Status |
|----|-------|--------|
| CI-01 | Pin GitHub Actions to commit SHAs | **Mitigated** |
| SNAP-01 | AppOpen hash ignores runtime | **Mitigated** — runtime swapfile/zram in hash |
| GUI-01 | AppOpen not wired in GUI | **Accepted** — GUI no longer auto-creates AppOpen (avoids startup polkit); PreApply on Apply remains |
| GUI-02 | Configure preview wiped by refreshAll | **Mitigated** |
| GUI-03 | Ungated `XZRAM_CLI` | **Mitigated** — `XZRAM_ALLOW_DEV_CLI` |
| GUI-04 | Swap on/off without confirm | **Mitigated** |
| PKG-06 | Qt hard Depends | **Mitigated** — `xzram` / `xzram-gui` split |
| DBUS-01 | Unauthenticated pending/snapshot reads | **Mitigated** — `store.read` + CLI EACCES→D-Bus |
| HELPER-02 | Sysctl range checks | **Mitigated** — `validate_sysctl_values` (0–200 / 0–10000 / 0–8) |
| GUI-05a | Prefer shared `last_error` over process stderr | **Mitigated** — process streams first |
| GUI-05b | Sync `QProcess` on UI thread | **Partial** — long ops (apply/restore/rollback/swapfile create-resize/AppOpen) use async `CliJob` + progress; timer refresh reads remain sync |
| GUI-06 | Doctor HTML escape / uint64 double parse | **Mitigated** |

### Still deferred

| ID | Topic | Notes |
|----|-------|-------|
| PKG-08 | Cargo vendor / offline builds | Distro packaging when required |
| GUI-05b-refresh | Async/parallel timer `refreshLive` reads | Follow-up after long-ops path |

---

## Release checklist (polkit / privileged)

Before tagging a release that touches helper, daemon, or polkit:

1. Run the manual matrix in [`scripts/polkit-smoke-checklist.sh`](../scripts/polkit-smoke-checklist.sh) on at least one Arch-family and one Debian-family host (Fedora when available).
2. Confirm unprivileged `xzram --dbus apply` (or GUI apply) prompts or denies — never silent success as root peer.
3. Smoke: `xzram status`, `detect`, `doctor` (no sudo); privileged apply only with intentional auth.
4. `make test-lib` and `make lint` green.

---

## Related documentation

- [SCOPE.md](SCOPE.md) — supported backends and out-of-scope items
- [RECOMMENDATIONS.md](RECOMMENDATIONS.md) — hardware/distro recommendation matrix
- [SNAPSHOTS.md](SNAPSHOTS.md) — snapshot / restore / GUI Snapshot tab
- [GUI-PHASE2.md](GUI-PHASE2.md) — GUI is CLI-backed; daemon optional for other clients
- [DEV-ENV.md](DEV-ENV.md) — `XZRAM_DATA_DIR` / `XZRAM_ETC_ROOT` for hermetic tests
