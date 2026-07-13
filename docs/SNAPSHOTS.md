# XZram Configuration Snapshots

XZram stores versioned configuration snapshots under `/var/lib/xzram/snapshots/`.

## When snapshots are created

| Trigger | When |
|---------|------|
| `app_open` | GUI startup (skipped if state unchanged since last snapshot) |
| `pre_apply` | Immediately before every `xzram apply` |
| `manual` | `xzram snapshot create` |

## What is captured

- `/etc/systemd/zram-generator.conf`
- `/etc/fstab`
- `/etc/sysctl.d/99-xzram.conf`
- `/etc/default/zramswap` (legacy zram-tools)
- Runtime metadata: active zram devices and swapfile paths/sizes

Swapfile **binary contents are not copied** (they may be gigabytes). Restore recreates missing swapfiles from recorded sizes when possible.

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

The **Utilities → Restore Snapshots** tab lists snapshots and allows restore. Deletion is CLI-only.

## Retention

Default retention keeps the **50 newest** snapshots. Prune with:

```bash
xzram snapshot prune --keep 50 --yes
```

## Legacy backup migration

The old single-directory backup at `/var/lib/xzram/backup/` is imported automatically as a manual snapshot on first run.
