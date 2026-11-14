# XZram Configuration Snapshots

XZram stores versioned configuration snapshots under `/var/lib/xzram/snapshots/`.

## When snapshots are created

| Trigger | When |
|---------|------|
| `app_open` | Explicit `xzram snapshot create --trigger app_open` only (GUI no longer auto-creates on startup). Skipped when `state_hash` matches the latest snapshot; hash covers managed `/etc` files plus canonical runtime swapfile/zram metadata. |
| `pre_apply` | Immediately before every `xzram apply` |
| `manual` | `xzram snapshot create` (default) |

## What is captured

- `/etc/systemd/zram-generator.conf`
- `/etc/fstab`
- `/etc/sysctl.d/99-xzram.conf`
- `/etc/default/zramswap` (legacy zram-tools)
- Runtime metadata: active zram devices and swapfile paths/sizes

Swapfile **binary contents are not copied** (they may be gigabytes). Restore recreates missing swapfiles from recorded sizes when possible.

Restore **merges** `/etc/fstab`: only swap entries are replaced with the snapshot's swap entries; other mounts added since the snapshot are kept. Swapfiles created after the snapshot (listed in fstab, or active and newer than the snapshot) are deactivated and removed, but only when they pass the same checks as `xzram swapfile remove` (managed path, no symlinks, root-owned parents); any other file is left in place and reported. All restored files are written atomically.

Restore swaps off zram and the swapfiles it manages, so their contents come back into RAM. Like `apply`, it first checks that `MemAvailable` can take that (plus a margin) and refuses with "not enough free memory" before changing anything. If a step fails after swapoff, swap is turned back on from the configuration then on disk before the error is returned. A snapshot taken when an fstab couldn't be read is refused at creation instead of recording "no swapfiles".

If `xzram apply` fails partway, the pre-apply snapshot is restored automatically and `pending.json` is kept so you can retry. Direct privileged actions (zram set/disable, swapfile create/resize/remove, swap on, sysctl set) also take a `pre_apply` snapshot first, so `xzram rollback` undoes the most recent change.

## CLI

```bash
xzram snapshot list
xzram snapshot create --label "Before experiment"
xzram snapshot restore last-apply
xzram snapshot restore <id>
xzram rollback                    # alias for restore last-apply
xzram snapshot delete <id> --yes  # destructive
xzram snapshot prune --keep 50 --yes
```

## GUI

The **Snapshot** tab lists snapshots and supports create, restore, delete, prune, and rollback (same actions as the CLI). Confirm-before-apply settings still gate destructive restore/delete/prune.

## Retention

Every new snapshot prunes the store to the **50 newest**. The newest `pre_apply` snapshot is always kept, however old, because `xzram rollback` restores it. Prune further by hand with (the same `pre_apply` rule applies):

```bash
xzram snapshot prune --keep 50 --yes
```

## Legacy backup migration

The old single-directory backup at `/var/lib/xzram/backup/` is imported automatically as a manual snapshot on first run.
