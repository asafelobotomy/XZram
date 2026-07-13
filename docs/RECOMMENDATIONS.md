# XZram recommendations reference

Hardware-aware defaults used by `xzram defaults recommend` and the Dashboard **Apply recommended defaults** action. Each rule maps to a `reference` ID in CLI/GUI output.

## Summary matrix

| RAM tier | Profile | `zram-size` | Algorithm | Sysctl | Disk overflow |
|----------|---------|-------------|-----------|--------|---------------|
| &lt; 4 GiB | `constrained` | `min(ram, 4096)` | `lz4` if &lt; 4 cores, else `zstd` | 180 / 0 / 125 / 0 | `/swap/swapfile`, RAM-sized, pri 10 |
| 4–31 GiB (default) | `conservative` | `min(ram / 2, 4096)` | `zstd` (Arch/CachyOS always `zstd`) | same | same |
| ≥ 32 GiB | `conservative` | `min(ram / 2, 8192)` | `zstd` | same | same |
| CachyOS | `performance` | `ram` | `zstd` | same | same + resident-limit advisory |

**Priority tiers:** zram `swap-priority = 100`, disk swapfile `pri = 10`.

**Anchor IDs:** `profile-constrained`, `profile-conservative`, `profile-performance`, `sysctl-tuning`, `overflow-swapfile`, `priority-tiers`.

## Distro overrides

| Distro / family | Override |
|-----------------|----------|
| **CachyOS** (`id = cachyos`) | `performance` profile: `zram-size = ram`, `zstd` ([CachyOS-Settings](https://github.com/CachyOS/CachyOS-Settings/blob/master/usr/lib/systemd/zram-generator.conf)) |
| **Arch family** | Always `zstd` compression |
| **Fedora / systemd default** | `min(ram / 2, 4096)` when no override ([zram-generator](https://github.com/systemd/zram-generator)) |

Anchor: `distro-overrides`.

## Doctor ↔ recommend mapping

| Doctor code | Recommend behavior |
|-------------|-------------------|
| `zswap_enabled` / `zram_zswap_conflict` | Advisory: disable zswap ([Arch Wiki Zswap](https://wiki.archlinux.org/title/Zswap)) |
| `hibernate_zram` | Advisory; zram staging skipped |
| `priority_inverted` | Fixed by staged zram pri 100 + swapfile pri 10 |
| `btrfs_swapfile_nodatacow` | Prepare runs before create via `ensure_ready_for_swapfile` |
| `zram_tools_legacy` | Advisory: run `xzram zram migrate` first |
| `no_zram` / `zram_generator_missing` | Informational |
| `zfs_root` | Advisory only |
| Algorithm mismatch (configured ≠ active) | Advisory ([Fedora kernel 6.12.5 quirk](https://discussion.fedoraproject.org/t/zram-zstd-algorithm-is-overriden-by-lzo-after-kernel-v6-12-5-update/140498)) |

Anchor: `doctor-mapping`.

## Known conflicts

1. **zswap + zram** — disable zswap when using zram ([Arch Wiki Zram](https://wiki.archlinux.org/title/Zram), [archinstall #1493](https://github.com/archlinux/archinstall/issues/1493)).
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
- XZram does not configure zswap; Doctor warns when zswap conflicts with zram.
- Anchor: `zswap-alternative`.

### Writeback device {#writeback-device}

- `writeback-device=` requires ongoing timer/daemon to flush pages ([systemd#164](https://github.com/systemd/zram-generator/issues/164)).
- XZram uses a low-priority overflow **swapfile** instead; simpler and automatic.
- Anchor: `writeback-device`.

### Implement vs document matrix {#advanced-matrix}

| Topic | Apply Defaults | Document | Future |
|-------|----------------|----------|--------|
| `zram-resident-limit` | Staged on `performance` profile | § resident-limit | — |
| Multiple zram devices | No | § multi-device | No |
| zswap alternative | Advisory | § zswap-alternative | No zswap UI |
| Writeback device | No (overflow swapfile) | § writeback-device | Expert mode |

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
| 2026-07-13 | Initial matrix: conservative/performance/constrained profiles, overflow swapfile, sysctl defaults, advanced topics, doctor mapping |
