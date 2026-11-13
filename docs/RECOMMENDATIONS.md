# XZram recommendations reference

Hardware-aware defaults used by `xzram defaults recommend` and the Dashboard **Apply recommended defaults** action. Each rule maps to a `reference` ID in CLI/GUI output.

## Summary matrix

| RAM tier | Profile | `zram-size` | Algorithm | Sysctl | Disk overflow |
|----------|---------|-------------|-----------|--------|---------------|
| &lt; 4 GiB | `constrained` | `min(ram, 4096)` | `lz4` if &lt; 4 cores, else `zstd` | 180 / 0 / 125 / 0 | `/swap/swapfile`, `min(RAM, 8192)` MiB, pri 10 |
| 4–11 GiB | `conservative` | `min(ram / 2, 4096)` | `zstd` (Arch/CachyOS always `zstd`) | same | same |
| 12–31 GiB (MemTotal ≥ 11 GiB) | `conservative` | `min(ram, 16384)` | `zstd` | same | same |
| ≥ 32 GiB | `conservative` | `min(ram / 2, 16384)` | `zstd` | same | same |
| CachyOS | `performance` | `ram` | `zstd` | same | same + resident-limit advisory |

**Why `min(ram, 16384)` for 12–31 GiB** (changed after 0.5.0, issues [#13](https://github.com/asafelobotomy/XZram/issues/13) / [#28](https://github.com/asafelobotomy/XZram/issues/28)): `zram-size` is the *uncompressed* capacity. With zstd's typical 2.5–3.5x ratio a full device of RAM size costs about a third of RAM, while the old `min(ram / 2, 4096)` (and Fedora's `min(ram, 8192)`) filled on a 15.5 GiB machine under parallel builds and sent the overflow to disk with 5–7 GiB of RAM still available (PSI memory `some` 28%, back to ~2.5% after growing zram). 12 GiB machines report ~11.5 GiB MemTotal, hence the 11 GiB cut. The 32 GiB tier keeps `ram / 2` but its cap rises to 16 GiB so a 32 GiB machine never gets a smaller device than a 31 GiB one. Existing larger sizes are never shrunk (see distro overrides).

Size tiers per RAM size (`--zram-scale`):

| MemTotal | Low | Default | High |
|----------|-----|---------|------|
| &lt; 11 GiB (≥ 4) | `min(ram / 4, 2048)` | `min(ram / 2, 4096)` | `min(ram, 8192)` |
| 11–31 GiB | `min(ram / 2, 4096)` | `min(ram, 16384)` | `min(ram * 1.5, 24576)` |
| ≥ 32 GiB | `min(ram / 4, 4096)` | `min(ram / 2, 16384)` | `min(ram, 32768)` |

**Saturation, memory guard, drain:** see § zram-saturation. **Btrfs swapfiles:** see § btrfs-swap-subvolume.

**Priority tiers:** zram `swap-priority = 100`, disk swapfile `pri = 10`.

**Sysctl (all profiles):** the zram set above (`swappiness` 180, `watermark_boost_factor` 0, `watermark_scale_factor` 125, `page-cluster` 0, from Pop!_OS via the [Arch Wiki](https://wiki.archlinux.org/title/Zram)) plus desktop knobs from [CachyOS-Settings](https://github.com/CachyOS/CachyOS-Settings): `vm.dirty_bytes` 256 MiB / `vm.dirty_background_bytes` 64 MiB (the default 20% / 10% ratios let gigabytes of dirty pages pile up, then stall on flush, worst on HDDs) and `vm.vfs_cache_pressure` 50. Anchor: `sysctl-tuning`.

**Overflow gates:** when the only active disk swap is an undersized swapfile at `/swap/swapfile` (e.g. a distro-installed 512 MiB file), Apply grows it in place to the target (`swapfile_resize`; never shrinks). Otherwise skipped when other active `/proc/swaps` disk swap exists, when fstab/managed inventory already lists non-zram swap, or when free space near `/swap` is below size + 512 MiB margin.

**Size scale (GUI / CLI):** `xzram defaults recommend|stage|apply` accept `--zram-scale` and `--swap-scale` (`low` | `default` | `high`). The Recommended defaults dialog exposes the same tiers as sliders. Recommended matches the matrix above; Low is more conservative; High raises caps (overflow High caps at 16 GiB). See `size_scales` in `--json` output.

**Linked optimize (GUI / CLI):** `xzram defaults optimize-linked --anchor <field> --json` takes a pending-shaped seed (stdin or `--seed-file`) and rewrites dependents to recommend-aligned values while preserving a valid anchor. Fatal inputs (bad zram expression, out-of-range sysctl, priority inversion) are replaced with the nearest safe recommendation. The GUI Settings toggle **Keep linked settings optimized** (`linkedOptimize`, default on) runs this live (debounced) across ZRAM, Sysctl, and Swapfile tabs so Stage persists the linked set.

**Staging hard stops:** read-only `/etc` or `immutable_os` (NixOS / ostree / Silverblue) → no zram, sysctl, or swapfile staging.

**Placement:** recommend and doctor (`swap_on_rotational`) warn when disk swap is, or would be created, on a rotational drive (sysfs `queue/rotational`). zram lives in RAM and is unaffected.

**Anchor IDs:** `profile-constrained`, `profile-conservative`, `profile-performance`, `sysctl-tuning`, `overflow-swapfile`, `priority-tiers`, `oom-protection`, `mm-tunables`, `zram-saturation`, `btrfs-swap-subvolume`.

**Doctor headline:** `healthy` (and exit code 3) still reflects errors only; the report also carries `errors`, `warnings` and `summary` ("System healthy", "Healthy, 1 warning", "Issues found: 1 error, 2 warnings"), which the CLI and GUI print as the headline.

## Distro overrides

| Distro / family | Override |
|-----------------|----------|
| **CachyOS** (`id = cachyos`) | `performance` profile: `zram-size = ram`, `zstd` ([CachyOS-Settings](https://github.com/CachyOS/CachyOS-Settings/blob/master/usr/lib/systemd/zram-generator.conf)) |
| **Arch family** | Always `zstd` compression |
| **Fedora / vendor defaults** | Do **not shrink** an existing generator size that already evaluates ≥ the recommended formula (e.g. keep `min(ram, 8192)` on 4–11 GiB). On 12–31 GiB the new default `min(ram, 16384)` is larger than Fedora's `min(ram, 8192)`, so recommend now proposes growing it there. Still stage algo / priority / sysctl / overflow as needed. Upstream default size is often `min(ram / 2, 4096)` ([zram-generator](https://github.com/systemd/zram-generator)); Fedora packages may ship `min(ram, 8192)`. Size formulas are evaluated with `+ - * /`, parentheses, `min`/`max` and decimals (e.g. `ram * 1.5`). |
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
| `zram_near_full` | zram ≥ 90% full with lower-priority disk swap in use. Warning; **error** (doctor unhealthy, exit 3) when the overflow is ≥ max(256 MiB, 10% of the device) and memory PSI shows stalls (full avg60 ≥ 5%, some avg60 ≥ 20% or some avg300 ≥ 10%). Recommend grows zram (see § saturation) |
| `swapfile_blocks_snapshots` | Error: active swapfile in a snapshotted btrfs subvolume. Recommend advises moving it (§ btrfs swap subvolume); overflow swapfile not staged/grown there |
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

### zram saturation {#zram-saturation}

- when a zram swap device is ≥ 90% full *and* lower-priority disk swap holds data, recommend (Default and High tiers) proposes a larger size instead of "already matches": `min(ram * m, 65536)` where `m` = measured `mm_stat` ratio (orig / compr, clamped to 1.5–4) × 0.5, rounded down to a quarter, so a completely full device costs at most ~50% of RAM (3.2x → `ram * 1.5`, 2.8x → `ram * 1.25`). Without a reported ratio it proposes the High tier. It never proposes less than the live device or the configured size (growth under 10% is skipped); when nothing larger fits, a note suggests zswap / more RAM instead. Doctor reports the same state as `zram_near_full`.
- Applying a zram change restarts `systemd-zram-setup@zramN`, which swaps everything stored in the device back into RAM first. `xzram apply` (and `zram set|disable --now`, D-Bus `ApplyNowZram`) therefore compares `MemAvailable` with the devices' stored data (`mm_stat` orig_data_size) plus a margin of 5% of RAM (at least 512 MiB) and refuses below it, changing nothing (`not enough free memory: ...`). `--at-boot` writes the generator config only (the running device changes at the next boot; with `disable`, only the empty override is written); `--force` restarts anyway. D-Bus: `ApplyWithOptions(b at_boot, b force)`, `ApplyZramWithOptions(s config, b at_boot, b force)`.
- Draining overflow (opt-in): after zram grows, what already spilled stays on disk until touched. `xzram swap drain-overflow [--dry-run] [--force]` runs `swapoff` + `swapon -p <same priority>` on each disk swap below zram's priority that holds data. Guard: estimated RAM cost (pages that fit in zram's free capacity cost their compressed size at the measured ratio, the rest their full size) plus the same margin must fit in `MemAvailable`. Helper action `swap.drain-overflow` (polkit `io.github.xzram.swap.drain`; pkexec only, not over D-Bus).
- Anchor: `zram-saturation`.

### Btrfs swap subvolume {#btrfs-swap-subvolume}

- btrfs refuses to snapshot a subvolume holding an active swapfile (kernel: "cannot snapshot subvolume with active swapfile", ETXTBSY), so a swapfile in the root subvolume silently breaks snapper timelines, snap-pac pre/post snapshots and Timeshift. A swapfile counts as misplaced when its subvolume (nearest ancestor with inode 256 on the file's device) is the one at `/`, at `/home`, or a `SUBVOLUME=` of any `/etc/snapper/configs/*`. Then: creating a swapfile there is refused (`swapfile create`, `defaults apply`), recommend does not stage or grow the overflow swapfile and explains why, doctor reports `swapfile_blocks_snapshots` (error), and recommend lists it as a host fix (advice only). XZram does not create subvolumes or mounts itself; the fix is a top-level `@swap` subvolume mounted at `/swap` (`mount -o subvolid=5 <dev> /mnt && btrfs subvolume create /mnt/@swap`, fstab `<dev> /swap btrfs subvol=/@swap,noatime 0 0`), then `xzram defaults apply` creates the swapfile there. A nested subvolume (`btrfs subvolume create /swap`) also avoids the error but is lost by a snapper rollback.
- Anchor: `btrfs-swap-subvolume`.

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
| Saturated zram | Larger ratio-based size staged | § zram-saturation | — |
| zram restart memory guard / `--at-boot` | Always on (`--force` to bypass) | § zram-saturation | — |
| Drain disk overflow | No (explicit `xzram swap drain-overflow`) | § zram-saturation | — |
| Swapfile in snapshotted btrfs subvolume | Refused; advice only | § btrfs-swap-subvolume | Create `@swap` automatically |

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
| Unreleased | Conservative 12–31 GiB default `min(ram, 16384)`, ≥ 32 GiB `min(ram / 2, 16384)`, new tier table; saturation-aware zram size (ratio-based); memory guard + `--at-boot` / `--force` for zram restarts; `swap drain-overflow`; doctor `zram_near_full` can be an error, `swapfile_blocks_snapshots`; btrfs swapfiles refused in snapshotted subvolumes |
| 2026-10-14 (0.4.1) | oomd `session.slice` guard; MGLRU `min_ttl_ms`, THP `defrag`, dirty bytes, `vfs_cache_pressure`; doctor `oomd_session_unprotected`, `thp_always_low_ram` |
| 2026-10-14 | Grow undersized managed `/swap/swapfile`; rotational placement warning; systemd-oomd OOM protection (drop-ins + enable); doctor RAM/swap checks (OOM kills, PSI, swap headroom, overflow size) |
| 2026-07-22 | Overflow capped at 8 GiB; fstab + free-space gates; RO `/etc` and immutable OS skip all staging; respect larger vendor zram-size; Apt package hint; GUI Apply vs Configure copy |
| 2026-07-13 | Initial matrix: conservative/performance/constrained profiles, overflow swapfile, sysctl defaults, advanced topics, doctor mapping |
