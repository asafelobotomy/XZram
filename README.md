# XZram

[![ko-fi](https://ko-fi.com/img/githubbutton_sm.svg)](https://ko-fi.com/U5R225QZH3)

Cross-distro Linux swap management: zram, swap files, partitions, and sysctl tuning.

XZram is a CLI-first tool for creating, removing, and customizing swap on systemd-based
Linux distributions. It includes a Qt6 GUI and system D-Bus daemon (`xzramd`).

## Features

- **Read-only diagnostics** — `status`, `detect`, and `doctor` without root
- **Staged apply** — write commands stage to `/var/lib/xzram/pending.json`; `apply` executes atomically
- **ZRAM management** — configure via `systemd-zram-generator`; migrate from legacy zram-tools
- **Swap file management** — create, resize, remove disk-backed swap (btrfs nodatacow enforced)
- **Swap partitions** — `swap list` merges active swaps and fstab partitions
- **Sysctl tuning** — swappiness, watermark, and page-cluster settings
- **Polkit integration** — granular privileged operations (no blanket sudo)
- **D-Bus daemon** — `xzramd` on `io.github.XZram1` for `--dbus` / other clients; D-Bus activation starts it on demand (native GUI does not auto-start or require it)
- **Qt6 GUI** — dashboard, zram, swap files, sysctl, doctor, snapshots, and settings (CLI/pkexec-first; daemon optional)
- **uQ Settings page** — on [uQ](https://github.com/asafelobotomy/uq) systems, a "Memory and swap" page under Apps: memory and swap in use, zram size and compression, swappiness, and the swap list. It can change the zram size and algorithm and swappiness (through xzramd). Optional: install `uq`; see [`uq/README.md`](uq/README.md)
- **Configuration snapshots** — automatic backups before every apply; versioned restore (`app_open` is manual)

## Supported distros

| Family | Detection / runtime | Packaging in this repo |
|--------|---------------------|------------------------|
| Fedora / RHEL / CentOS Stream | Full | [`packaging/xzram.spec`](packaging/xzram.spec) |
| Debian / Ubuntu / derivatives | Full | [`debian/`](debian/) |
| Arch / CachyOS / Manjaro | Full | [`packaging/aur/`](packaging/aur/) (AUR) · [`PKGBUILD`](PKGBUILD) (local) |
| openSUSE | Full | Packaging TBD (detection supported) |
| Gentoo | Partial | — |
| NixOS / Alpine / non-systemd | Out of scope v1 | — |

Published distro packages are not assumed until an AUR/release is cut; use From source or the
in-tree packaging files (`packaging/aur/`, `debian/`, `packaging/xzram.spec`) when building locally.

The release version is recorded in [`VERSION`](VERSION) and [`CHANGELOG.md`](CHANGELOG.md).
Keep packaging (`PKGBUILD`, `packaging/xzram.spec`, `debian/changelog`, AppStream) and
`gui/CMakeLists.txt` in sync — `make version-check` verifies this.

## Prerequisites

- **CLI / daemon:** Rust ≥ 1.87, Cargo, Linux with systemd
- **Full install / GUI:** cmake, Qt6 Widgets
  - Arch: `qt6-base`, `cmake`
  - Debian/Ubuntu: `qt6-base-dev`, `cmake`, `pkg-config`
  - Fedora: `qt6-qtbase-devel`, `cmake`

## Quick start

```bash
# Build from source (release; slow cold build — fine for packaging)
cargo build --release

# Faster iteration while developing
cargo run -p xzram-cli -- status
make check
make test-lib

# Read-only commands (no root required)
./target/release/xzram status
./target/release/xzram detect
./target/release/xzram doctor

# Stage changes (default) then apply (apply needs polkit / root via helper)
./target/release/xzram zram set --size "min(ram / 2, 4096)"
./target/release/xzram apply

# Immediate apply for scripting
./target/release/xzram zram set --now
```

Do **not** use `sudo` for smoke checks of `status` / `detect` / `doctor`.

## Verify

```bash
make lint          # fmt --check + clippy -D warnings + loc-check (matches CI)
make test-lib      # fast unit tests for crates/xzram
make test          # full cargo test workspace
make gui-smoke     # Qt6 offscreen launch smoke (needs GUI deps)
```

Diagnostics: `xzram status` and `xzram doctor` are the operator health surface (no root).
For binary tracing: `RUST_LOG=debug cargo run -p xzram-cli -- status` (see [docs/DEV-ENV.md](docs/DEV-ENV.md)).

Optional local git hooks (same gates as CI lint + lib tests):

```bash
pip install pre-commit   # or distro package: pre-commit
pre-commit install
```

See [AGENTS.md](AGENTS.md) for agent-oriented bootstrap notes and
[docs/DEV-ENV.md](docs/DEV-ENV.md) for test overrides (`XZRAM_*`).

## Installation

### From source (recommended for checkouts)

```bash
# CLI + daemon + polkit/D-Bus (no Qt): build as your user, copy as root
make build
sudo make install-cli-files

# Or full install including xzram-qt (requires cmake + Qt6)
make build build-gui
sudo make install-files

# Optional: enable xzramd for --dbus / other clients (D-Bus activation also starts it on demand)
sudo systemctl enable --now xzramd
```

`make install-cli` also installs the uQ Settings page to `$(DATADIR)/uq/settings/pages/xzram/`. It does nothing unless uQ is installed.

xzram installs under `/usr` only: the polkit policy, the D-Bus files and `xzramd.service` name
`/usr/libexec`, and polkit and the system bus only read `/usr/share`, so the install targets
refuse another `PREFIX`, `BINDIR` or `LIBEXECDIR` (use `DESTDIR` to stage). `sudo make uninstall`
removes what `install-files` installed and keeps `/var/lib/xzram` (snapshots).

The `*-files` targets only copy what is already built, so `target/` and `build-gui/`
never become root-owned, and `install` gives every installed file root ownership and an
explicit mode. Don't stage with `DESTDIR` and then `cp -a` the tree onto `/`: that keeps your
user as the owner of `/usr/libexec/xzram-helper`, which pkexec runs as root. For reinstall/uninstall helpers see
[`scripts/reinstall-system.sh`](scripts/reinstall-system.sh) and
[`scripts/uninstall-system.sh`](scripts/uninstall-system.sh). The native GUI uses
CLI + pkexec and does not require `xzramd`.

### Build GUI only

```bash
make build-gui
# Binary: build-gui/xzram-qt/xzram-qt
```

### Arch / CachyOS / AUR

- **Local checkout:** root [`PKGBUILD`](PKGBUILD) (`file://$startdir`) for packaging experiments from a clone.
- **AUR:** [`packaging/aur/`](packaging/aur/) — split package `xzram` + `xzram-gui` from the `v$pkgver` GitHub source archive.

```bash
# After the package is published on the AUR:
yay -S xzram          # CLI / helper / daemon
yay -S xzram-gui      # Qt6 GUI (depends on xzram)
```

The `xzram` package lists `uq` as an optional dependency for its Memory and swap page in uQ Settings; the page is installed with the package either way.

Release CI (`.github/workflows/aur-release.yml`) builds the AUR packages in an Arch container and, on a published GitHub Release (`v*`), uploads `.pkg.tar.zst` plus pinned `PKGBUILD` / `.SRCINFO` to that release. First-time AUR publish is still a manual `git push` to `ssh://aur@aur.archlinux.org/xzram.git` using those files (see [`packaging/aur/README.md`](packaging/aur/README.md)).

### Debian

```bash
dpkg-buildpackage -us -uc -b
sudo dpkg -i ../xzram_*.deb ../xzram-gui_*.deb   # GUI optional
# xzramd is D-Bus-activated; enable optional:
# sudo systemctl enable --now xzramd
```

### Fedora

Build from [`packaging/xzram.spec`](packaging/xzram.spec) with `rpmbuild` (or your
usual RPM workflow). Packages: `xzram` (CLI/daemon) and `xzram-gui` (Qt).

## CLI reference

`xzram <command> --help` lists every flag. Commands that change the system stage the change
by default (`xzram apply` makes it live); `--now` applies it immediately.

```
# Read-only (no root)
xzram status              # all swap devices, zram stats, priorities, memory/io PSI
xzram doctor              # health checks; headline "System healthy" / "Healthy, N warnings" / "Issues found: ..."
xzram detect              # distro, backend, installed packages

# zram
xzram zram show
xzram zram set [--device zram0] [--size EXPR] [--algorithm ALGO] [--priority N] [--now] [--at-boot|--force]
xzram zram set --size 'min(ram, 16384)' --now      # restart zram now (memory-checked)
xzram zram set --size 'min(ram, 16384)' --at-boot  # write config; takes effect at next boot
xzram zram disable [--now] [--at-boot|--force]
xzram zram migrate [--now]                         # from legacy zram-tools configuration

# Swap files and devices
xzram swapfile list
xzram swapfile create PATH --size-mb N [--priority N] [--prepare] [--mkdir] [--now]
xzram swapfile resize PATH --size-mb N [--now]
xzram swapfile remove PATH [--now]
xzram swapfile check PATH                          # btrfs nodatacow readiness
xzram swapfile prepare PATH                        # chattr +C on the btrfs parent directory
xzram swap list                                    # active swaps plus fstab partitions
xzram swap on|off DEVICE
xzram swap drain-overflow [--dry-run] [--force]    # move disk-swap overflow back into zram

# Sysctl
xzram sysctl show
xzram sysctl set [--swappiness N] [--watermark-boost-factor N] [--watermark-scale-factor N]
                 [--page-cluster N] [--vfs-cache-pressure N] [--dirty-bytes N]
                 [--dirty-background-bytes N] [--zram-defaults] [--now]

# Recommended defaults
xzram defaults recommend [--zram-scale X] [--swap-scale X]   # review only, stages nothing
xzram defaults stage [--zram-scale X] [--swap-scale X]
xzram defaults apply [--yes] [--zram-scale X] [--swap-scale X] [--at-boot|--force]
xzram defaults optimize-linked --anchor FIELD [--seed-file FILE]  # used by the GUI; no polkit

# Staged changes
xzram pending show|clear
xzram apply [--at-boot|--force]  # --at-boot: zram changes wait for the next boot (no swapoff now)
                                 # --force: restart zram even if free memory can't take its data

# Snapshots
xzram snapshot list
xzram snapshot create [--label TEXT] [--trigger manual|app_open]
xzram snapshot restore ID|latest|last-apply
xzram snapshot delete ID [--yes]
xzram snapshot prune [--keep N] [--yes]  # default keep: 50
xzram rollback                           # restore the last pre-apply snapshot

xzram daemon start        # enable and start xzramd (pkexec)

Global flags: --json, --dbus
```

Changing zram (size, algorithm, disable) restarts the device, which first moves everything
stored in it back into RAM. `apply`, `zram set --now` and `zram disable --now` refuse
("not enough free memory", nothing changed) when `MemAvailable` is below the stored data
plus a margin; use `--at-boot` to defer the change to the next boot or `--force` to go
ahead. `xzram swap drain-overflow` is opt-in and uses the same check. On btrfs, swapfiles
are refused inside a snapshotted subvolume (the root subvolume, `/home`, or any snapper
config): btrfs cannot snapshot a subvolume with an active swapfile. See
[docs/RECOMMENDATIONS.md](docs/RECOMMENDATIONS.md) (§ zram-saturation, § btrfs-swap-subvolume).

### Exit codes

| Code | Meaning |
|------|---------|
| `0` | Success |
| `1` | Error (operation failed, authorization cancelled/denied, D-Bus failure, ...) |
| `2` | Usage error (invalid arguments) |
| `3` | `xzram doctor` completed but reported the system as **unhealthy**, i.e. at least one error (the report is still printed; with `--json` it is valid JSON with `healthy`, `errors`, `warnings` and `summary`). Warnings alone keep exit `0` |

With `--dbus`, a privileged command falls back to pkexec only when xzramd cannot be
reached *before* the request is sent. If xzramd stops responding mid-call, the command
fails (exit `1`) and asks you to check `xzram status` instead of re-running the operation.

See [docs/SNAPSHOTS.md](docs/SNAPSHOTS.md) for snapshot semantics and retention.
See [docs/RECOMMENDATIONS.md](docs/RECOMMENDATIONS.md) for recommended defaults.

## Architecture

```
xzram CLI / xzram-qt GUI
        │
        ├── read-only ──► xzram lib
        ├── pkexec ──► xzram-helper ──► xzram lib
        └── --dbus ──► xzramd (D-Bus) ──► xzram lib
```

See [docs/SCOPE.md](docs/SCOPE.md) for project scope and
[docs/GUI-PHASE2.md](docs/GUI-PHASE2.md) for the Qt6 GUI and D-Bus daemon architecture.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
