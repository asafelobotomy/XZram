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
| S-15 | `/var/lib/xzram` artifact modes | **Mitigated** — data dir `0o700`, pending/index/last_error `0o600` |
| S-16 | GUI JSON error escaping incomplete | **Mitigated** — `QJsonDocument`/`QJsonObject` in `xzramcli.cpp` |
| S-17 | Immutable-OS coverage (SteamOS) | **Mitigated** — steamos id/VARIANT + `steamos-readonly` / `steamos-release` |
| S-18 | Bus policy allows all users to call Manager | **Accepted** — writes still polkit-gated |
| S-19 | D-Bus mutators authorize caller | **Mitigated** — `Subject::new_for_message_header` (historical P0-1) |
| Q-01 | `snapshot/restore.rs` untested | **Mitigated** — hermetic restore tests under `XZRAM_ETC_ROOT` |
| Q-02 | `recommend/engine.rs` untested | **Mitigated** — `recommend_from_context` + unit tests |
| Q-03 | Stale `docs/AUDIT.md` | **Mitigated** — this living register |
| Q-04 | No Recommends for zram-generator | **Mitigated** — PKGBUILD optdepends; debian Recommends; RPM Recommends |

### Historical P0 (July 2026)

Caller-bound polkit, migrate action, btrfs auto-prepare, staged disable, empty-override disable — remain fixed.

---

## Packaging

| Artifact | CLI | Helper | Daemon | GUI | zram-generator hint |
|----------|-----|--------|--------|-----|---------------------|
| `PKGBUILD` | yes | yes | yes | yes | `optdepends=('systemd-zram-generator')` |
| `debian/` | yes | yes | yes | yes | `Recommends: systemd-zram-generator` |
| `packaging/xzram.spec` | yes | yes | yes | yes | `Recommends: zram-generator` |

Flatpak: **removed** (see [SCOPE.md](SCOPE.md)).

---

## Third-pass findings (2026-08-11)

Surfaces newly reviewed: GUI/CLI argv, packaging/CI/install, polkit/D-Bus policy, migrate/sysctl/swap listing, recommend overflow.

### Privilege / migrate (batch A — committed earlier)

| ID | Topic | Status |
|----|-------|--------|
| T3-01 | pkexec of non-annotated helper / `XZRAM_DEV_HELPER` | **Mitigated** — annotated path only; dev override gated |
| T3-02 | `systemd-run` env expansion of helper argv | **Mitigated** — `--expand-environment=no` |
| T3-03 | Helper timeout without unit RuntimeMaxSec | **Mitigated** — `RuntimeMaxSec=300` on transient unit |
| T3-M01 | Finalize zram-tools on any apply | **Mitigated** — `pending.finalize_zram_tools` from migrate only |
| T3-M02 | Migrate finalize ignores systemctl failure | **Mitigated** — propagate disable errors |
| T3-D01 | Hibernate-on-zram shown healthy | **Mitigated** — severity Error |
| T3-H02 | `prune --keep 0` deletes all snapshots | **Mitigated** — reject keep=0 |
| T3-deb | Debian prerm disable-on-upgrade / invalid start | **Mitigated** — prerm/postinst cleaned |

### Correctness / packaging (batch B — final unaudited surfaces)

| ID | Topic | Status |
|----|-------|--------|
| SYSCTL-01 | Partial sysctl `--now` overwrites drop-in | **Mitigated** — merge-write existing keys |
| MIG-01 | Migrate ignores `SIZE`/`PRIORITY` | **Mitigated** — parse SIZE (MiB) + PRIORITY |
| MIG-02 | `eval_zram_size_mb` misses percent formulas | **Mitigated** — `ram/100*N` + absolute MiB |
| SWAP-01 | One bad UUID aborts partition list | **Mitigated** — soft-fail per entry |
| SWAP-02 | Active match misses by-uuid vs `/dev/sdX` | **Mitigated** — canonicalize compare |
| REC-01 | Overflow staged when `df` fails | **Mitigated** — skip when free space unknown |
| PKG-01 | PKGBUILD `source=…/..` packs parent dir | **Mitigated** — `file://$startdir` |
| PKG-03 | Debian/RPM/`make` omit `--locked` | **Mitigated** |
| PKG-04/05 | RPM missing `%preun` / `%license` | **Mitigated** |
| PKG-07 | Debian enables xzramd on install | **Mitigated** — `--no-enable --no-start` |
| PKG-09 | Wrong upstream homepage URL | **Mitigated** — `asafelobotomy/XZram` |
| CI-02/03/06 | permissions / macOS / `--locked` | **Mitigated** |
| CI-04 | Debian CI mounts source RW | **Mitigated** — `:ro` + writable copy |
| INST-01 | `PREFIX` vs hardcoded `/usr/libexec` | **Documented** — install with `PREFIX=/usr` |
| INST-02 | `install-post` under `DESTDIR` | **Mitigated** — refuse `DESTDIR` |
| INST-03 | Missing `StateDirectory=xzram` | **Mitigated** |
| INST-04 | bash-completion missing `snapshot` | **Mitigated** |
| GUI-01 | Docs claim AppOpen on GUI startup | **Documented** — not wired; docs corrected |

### Open follow-ups (not blocking)

| ID | Topic | Notes |
|----|-------|-------|
| CI-01 | Pin GitHub Actions to commit SHAs | Tags still floating; permissions least-privilege applied |
| SNAP-01 | AppOpen hash ignores runtime swap/zram | Config-only by design until GUI wires AppOpen |
| GUI-02..06 | Preview wipe, `XZRAM_CLI` trust, swap confirm, HTML escape | GUI polish backlog |
| PKG-06/08 | Qt hard Depends / no cargo vendor | Distro packaging follow-up |
| DBUS-01 | Unauthenticated read of pending/snapshots | Accepted with S-18; writes still polkit |
| HELPER-02 | Sysctl range checks | GUI caps; CLI accepts `u32` |

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
