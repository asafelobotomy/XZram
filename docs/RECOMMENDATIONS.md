# XZram recommendations reference

Hardware-aware defaults used by `xzram defaults recommend` and the Dashboard **Apply recommended defaults** action. Each rule maps to a `reference` ID in CLI/GUI output.

## Summary matrix

| RAM tier | Profile | `zram-size` | Algorithm | Sysctl | Disk overflow |
|----------|---------|-------------|-----------|--------|---------------|
| &lt; 4 GiB | `constrained` | `min(ram, 4096)` | `lz4` if &lt; 4 cores, else `zstd` | 180 / 0 / 125 / 0 | `/swap/swapfile`, `min(RAM, 8192)` MiB, pri 10 |
| 4–31 GiB (default) | `conservative` | `min(ram / 2, 4096)` | `zstd` (Arch/CachyOS always `zstd`) | same | same |
| ≥ 32 GiB | `conservative` | `min(ram / 2, 8192)` | `zstd` | same | same |
| CachyOS | `performance` | `ram` | `zstd` | same | same + resident-limit advisory |

**Priority tiers:** zram `swap-priority = 100`, disk swapfile `pri = 10`.

**Sysctl (all profiles):** the zram set above (`swappiness` 180, `watermark_boost_factor` 0, `watermark_scale_factor` 125, `page-cluster` 0, from Pop!_OS via the [Arch Wiki](https://wiki.archlinux.org/title/Zram)) plus desktop knobs from [CachyOS-Settings](https://github.com/CachyOS/CachyOS-Settings): `vm.dirty_bytes` 256 MiB / `vm.dirty_background_bytes` 64 MiB (the default 20% / 10% ratios let gigabytes of dirty pages pile up, then stall on flush, worst on HDDs) and `vm.vfs_cache_pressure` 50. Anchor: `sysctl-tuning`.

**Overflow gates:** when the only active disk swap is an undersized swapfile at `/swap/swapfile` (e.g. a distro-installed 512 MiB file), Apply grows it in place to the target (`swapfile_resize`; never shrinks). Otherwise skipped when other active `/proc/swaps` disk swap exists, when fstab/managed inventory already lists non-zram swap, or when free space near `/swap` is below size + 512 MiB margin.

**Size scale (GUI / CLI):** `xzram defaults recommend|stage|apply` accept `--zram-scale` and `--swap-scale` (`low` | `default` | `high`). The Recommended defaults dialog exposes the same tiers as sliders. Recommended matches the matrix above; Low is more conservative; High raises caps (overflow High caps at 16 GiB). See `size_scales` in `--json` output.

**Linked optimize (GUI / CLI):** `xzram defaults optimize-linked --anchor <field> --json` takes a pending-shaped seed (stdin or `--seed-file`) and rewrites dependents to recommend-aligned values while preserving a valid anchor. Fatal inputs (bad zram expression, out-of-range sysctl, priority inversion) are replaced with the nearest safe recommendation. The GUI Settings toggle **Keep linked settings optimized** (`linkedOptimize`, default on) runs this live (debounced) across ZRAM, Sysctl, and Swapfile tabs so Stage persists the linked set.

**Staging hard stops:** read-only `/etc` or `immutable_os` (NixOS / ostree / Silverblue) → no zram, sysctl, or swapfile staging.

**Placement:** recommend and doctor (`swap_on_rotational`) warn when disk swap is, or would be created, on a rotational drive (sysfs `queue/rotational`). zram lives in RAM and is unaffected.

**Anchor IDs:** `profile-constrained`, `profile-conservative`, `profile-performance`, `sysctl-tuning`, `overflow-swapfile`, `priority-tiers`, `oom-protection`, `mm-tunables`.

## Distro overrides

| Distro / family | Override |
|-----------------|----------|
| **CachyOS** (`id = cachyos`) | `performance` profile: `zram-size = ram`, `zstd` ([CachyOS-Settings](https://github.com/CachyOS/CachyOS-Settings/blob/master/usr/lib/systemd/zram-generator.conf)) |
| **Arch family** | Always `zstd` compression |
| **Fedora / vendor defaults** | Do **not shrink** an existing generator size that already evaluates ≥ the conservative formula (e.g. keep `min(ram, 8192)`). Still stage algo / priority / sysctl / overflow as needed. Upstream default size is often `min(ram / 2, 4096)` ([zram-generator](https://github.com/systemd/zram-generator)); Fedora packages may ship `min(ram, 8192)`. |
| **Debian / Ubuntu (Apt)** | Install package hint is `systemd-zram-generator` (not `zram-generator`) |

Anchor: `distro-overrides`.

## Doctor ↔ recommend mapping

| Doctor code | Recommend behavior |
|-------------|-------------------|
| `zswap_enabled` / `zram_zswap_conflict` | Staged when zram is in use: Apply writes `/etc/tmpfiles.d/xzram-zswap.conf` and turns zswap off at runtime; snapshot restore removes the drop-in and re-enables zswap ([Arch Wiki Zswap](https://wiki.archlinux.org/title/Zswap)) |
| `hibernate_zram` | Advisory; zram staging skipped |
| `priority_inverted` | Fixed by staged zram pri 100 + swapfile pri 10 when those changes are in scope |
| `btrfs_swapfile_nodatacow` | Prepare runs before create via `ensure_ready_for_swapfile`; for existing allowlisted swapfiles whose parent lacks nodatacow, Apply stages `prepare_swapfiles` (chattr +C on the parent; not reverted by restore) |
| `no_oom_protection` / `oomd_no_policies` | Staged as `enable_oomd` when no userspace OOM killer can act (see § oom-protection); advisory when systemd-oomd is missing, PSI is unavailable, or staging is blocked |
| `oomd_session_unprotected` | Staged as `enable_oomd` (adds the `session.slice` guard) when oomd kills inside `user@.service` but the session slice is not `avoid`/`omit` |
| `thp_always_low_ram` | Doctor info only (THP `always` on &lt; 16 GiB RAM): suggest `transparent_hugepage=madvise` on the kernel cmdline |
| `oom_kills` | Advisory note: kernel OOM kills in this or the previous boot (`/proc/vmstat`, journal) |
| `disk_overflow_small` / `no_disk_overflow` | Overflow swapfile grown / created (see overflow gates) |
| `swap_nearly_full` / `memory_pressure_high` / `memory_pressure_elevated` | Doctor only (live swap use ≥ 80%, PSI full avg60 ≥ 10% / some avg300 ≥ 10%) |
| `swap_on_rotational` | Advisory: move disk swap to SSD/NVMe |
| `zram_tools_legacy` | Advisory: run `xzram zram migrate` first (Apply does not migrate) |
| `no_zram` / `zram_generator_missing` | Informational; package name from package manager |
| `zfs_root` | Advisory only |
| Algorithm mismatch (configured ≠ active) | Advisory ([Fedora kernel 6.12.5 quirk](https://discussion.fedoraproject.org/t/zram-zstd-algorithm-is-overriden-by-lzo-after-kernel-v6-12-5-update/140498)) |
| Immutable / RO `/etc` | Hard stop: empty pending |

Anchor: `doctor-mapping`.

## Known conflicts

1. **zswap + zram** — disable zswap when using zram ([Arch Wiki Zram](https://wiki.archlinux.org/title/Zram), [archinstall #1493](https://github.com/archlinux/archinstall/issues/1493)). Apply defaults disables zswap (runtime + tmpfiles.d) when zram is in use.
2. **Hibernation** — resume device cannot be zram ([Arch Wiki Zram](https://wiki.archlinux.org/title/Zram)).
3. **Dual-tier swap** — zram pri 100 + disk pri 10 is a safety net; sustained heavy swap may suit zswap better ([LinuxBlog](https://linuxblog.io/zswap-better-than-zram), [Chris Down](https://chrisdown.name/2026/03/24/zswap-vs-zram-when-to-use-what.html)). Anchor: `dual-tier-tradeoff`.
4. **Kernel lzo-rle override** — generator may say `zstd` while active algo differs; check Doctor / ZRAM tab.

Anchor: `known-conflicts`.

## Advanced topics

### `zram-resident-limit` / `mem_limit` {#resident-limit}

- `zram-size` = uncompressed logical capacity; `zram-resident-limit` = cap on RAM used for compressed pages ([kernel zram docs](https://docs.kernel.org/admin-guide/blockdev/zram.html), [zram-generator.conf(5)](https://manpages.ubuntu.com/manpages/questing/man5/zram-generator.conf.5.html)).
- On `performance` profile, XZram stages `zram-resident-limit = ram / 2` alongside `zram-size = ram` as a safety valve.
- Anchor: `resident-limit`.

### Multiple zram devices {#multi-device}

- Modern distros use a single `zram0` for swap. Per-CPU multi-device setups are legacy ([Arch BBS](https://bbs.archlinux.org/viewtopic.php?id=247725)).
- XZram manages swap on `zram0` only; other devices (e.g. `/tmp` ramdisk) are out of scope.
- Anchor: `multi-device`.

### zswap as alternative {#zswap-alternative}

- Prefer **zram** when swap demand stays within ~20–30% of RAM ([LinuxBlog](https://linuxblog.io/zswap-better-than-zram)).
- Prefer **zswap** when swap is heavy, spiky, or NVMe-backed ([Chris Down](https://chrisdown.name/2026/03/24/zswap-vs-zram-when-to-use-what.html)).
- XZram only turns zswap off when zram is in use; it never enables or tunes zswap.
- Anchor: `zswap-alternative`.

### Writeback device {#writeback-device}

- `writeback-device=` requires ongoing timer/daemon to flush pages ([systemd#164](https://github.com/systemd/zram-generator/issues/164)).
- XZram uses a low-priority overflow **swapfile** instead; simpler and automatic.
- Anchor: `writeback-device`.

### OOM protection (systemd-oomd) {#oom-protection}

- When RAM, zram and disk swap run out, the kernel OOM killer fires only after the system has thrashed, often for minutes with a frozen desktop. systemd-oomd acts earlier using PSI and swap usage ([systemd-oomd(8)](https://www.freedesktop.org/software/systemd/man/latest/systemd-oomd.service.html), [oomd.conf(5)](https://www.freedesktop.org/software/systemd/man/latest/oomd.conf.html)).
- systemd-oomd only watches units with `ManagedOOMSwap=kill` / `ManagedOOMMemoryPressure=kill`. Fedora (`systemd-oomd-defaults`) and Ubuntu ship these; **Arch / Manjaro do not**, so enabling the service alone does nothing.
- When no userspace killer can act (earlyoom / nohang count as protected), Apply writes `/etc/systemd/system/-.slice.d/10-xzram-oomd.conf` (`ManagedOOMSwap=kill`) and `/etc/systemd/system/user@.service.d/10-xzram-oomd.conf` (`ManagedOOMMemoryPressure=kill`, limit 50%), mirroring Fedora's defaults, then runs `systemctl enable --now systemd-oomd`.
- **Session guard:** a pressure kill on `user@.service` picks a cgroup inside the user manager, which can be the compositor/shell. Ubuntu 26.10 dropped that policy after it ended whole sessions ([It's FOSS](https://itsfoss.com/news/ubuntu-26-10-oom-policy/)). XZram keeps it but also writes `/etc/systemd/user/session.slice.d/10-xzram-oomd.conf` (`ManagedOOMPreference=avoid`, honoured for user cgroups since systemd 252), so apps in `app.slice` are killed first, and reloads running user managers. Systems already protected by distro policies get only this guard staged.
- Snapshot restore removes the drop-ins and disables systemd-oomd again if it was disabled when the snapshot was taken.
- Requires PSI (`/proc/pressure/memory`); without it the recommendation suggests earlyoom.
- Anchor: `oom-protection`.

### Kernel memory tunables {#mm-tunables}

- Persisted in `/etc/tmpfiles.d/xzram-mm.conf` (`w-` lines, applied early at boot) and written to sysfs on apply. Snapshots record the live values; restore removes the drop-in and writes the old values back.
- **MGLRU `min_ttl_ms = 1000`** (staged when `/sys/kernel/mm/lru_gen` is enabled and the value is 0): protects the last second of working set; if it cannot stay resident the kernel OOM-kills instead of thrashing. Kernel docs: 1000 "usually eliminates intolerable janks", 3000 risks premature OOM kills ([multigen_lru](https://docs.kernel.org/admin-guide/mm/multigen_lru.html)). openSUSE Leap 16 ships 1000.
- **THP `defrag = defer+madvise`** (staged only when the kernel default `madvise` or `always` is active): compaction runs in the background except for `MADV_HUGEPAGE` regions ([transhuge](https://docs.kernel.org/admin-guide/mm/transhuge.html)); CachyOS ships it.
- THP `enabled` is a boot-time choice and is only advised by doctor (`thp_always_low_ram`).
- Anchor: `mm-tunables`.

### Implement vs document matrix {#advanced-matrix}

| Topic | Apply Defaults | Document | Future |
|-------|----------------|----------|--------|
| `zram-resident-limit` | Staged on `performance` profile | § resident-limit | — |
| Multiple zram devices | No | § multi-device | No |
| zswap alternative | Advisory (zswap disabled when zram is used) | § zswap-alternative | No zswap UI |
| Writeback device | No (overflow swapfile) | § writeback-device | Expert mode |
| Vendor size respect | Keep ≥ recommended size | § distro-overrides | — |
| Overflow cap 8 GiB | Yes | § overflow gates | — |
| systemd-oomd policies | Staged when no OOM killer can act | § oom-protection | — |
| oomd session guard | Staged when user@ pressure kill lacks it | § oom-protection | — |
| MGLRU min_ttl_ms / THP defrag | Staged (tmpfiles.d) | § mm-tunables | — |
| Dirty writeback / vfs_cache_pressure | Staged (sysctl) | § summary | — |

## Citations

**Official / upstream**

- [Arch Wiki — Zram](https://wiki.archlinux.org/title/Zram)
- [Arch Wiki — Zswap](https://wiki.archlinux.org/title/Zswap)
- [systemd/zram-generator](https://github.com/systemd/zram-generator)
- [Kernel zram admin guide](https://docs.kernel.org/admin-guide/blockdev/zram.html)
- [Ubuntu zram-generator.conf(5)](https://manpages.ubuntu.com/manpages/questing/man5/zram-generator.conf.5.html)

**Distro defaults**

- [CachyOS zram-generator.conf](https://github.com/CachyOS/CachyOS-Settings/blob/master/usr/lib/systemd/zram-generator.conf)
- [Fedora zram defaults discussion](https://discussion.fedoraproject.org/t/make-all-of-ram-be-zram/130286)

**Community / forums**

- [Garuda forum — swappiness & zram](https://forum.garudalinux.org/t/i-cant-switch-to-sysctl-vm-swappiness-10/33626/6)
- [Arch BBS — ZRAM sizing & algorithms](https://bbs.archlinux.org/viewtopic.php?id=247725)
- [Fedora — zstd overridden by lzo-rle](https://discussion.fedoraproject.org/t/zram-zstd-algorithm-is-overriden-by-lzo-after-kernel-v6-12-5-update/140498)
- [r/Fedora zram tuning benchmarks](https://www.reddit.com/r/Fedora/comments/mzun99/new_zram_tuning_benchmarks)
- [archinstall #1493](https://github.com/archlinux/archinstall/issues/1493)

**Guides / blogs**

- [Linux Dynamics — Fedora zRAM sizing](https://linuxdynamics.com/how-to-permanently-increase-zram-size-on-fedora-kde-plasma)
- [zram-tuning algorithm comparison](https://github.com/reapercanuk39/zram-tuning/blob/main/docs/algorithm-comparison.md)
- [Chris Down — Debunking zswap/zram myths](https://chrisdown.name/2026/03/24/zswap-vs-zram-when-to-use-what.html)
- [LinuxBlog — zswap better than zram](https://linuxblog.io/zswap-better-than-zram)
- [systemd/zram-generator#164 — writeback needs daemon](https://github.com/systemd/zram-generator/issues/164)

## Changelog

| Date | Change |
|------|--------|
| 2026-10-14 (0.4.1) | oomd `session.slice` guard; MGLRU `min_ttl_ms`, THP `defrag`, dirty bytes, `vfs_cache_pressure`; doctor `oomd_session_unprotected`, `thp_always_low_ram` |
| 2026-10-14 | Grow undersized managed `/swap/swapfile`; rotational placement warning; systemd-oomd OOM protection (drop-ins + enable); doctor RAM/swap checks (OOM kills, PSI, swap headroom, overflow size) |
| 2026-07-22 | Overflow capped at 8 GiB; fstab + free-space gates; RO `/etc` and immutable OS skip all staging; respect larger vendor zram-size; Apt package hint; GUI Apply vs Configure copy |
| 2026-07-13 | Initial matrix: conservative/performance/constrained profiles, overflow swapfile, sysctl defaults, advanced topics, doctor mapping |
