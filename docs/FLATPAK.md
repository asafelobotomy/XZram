# XZram Flatpak Guide

## Distribution model

The Flatpak GUI cannot write `/etc` directly. Install the native host package first:

```bash
# Arch
pacman -S xzram

# Fedora
dnf install xzram

# Debian/Ubuntu
apt install xzram
```

This provides `xzramd`, `xzram-helper`, polkit policy, and snapshot storage.

## Flatpak permissions

The Flatpak manifest must include:

```yaml
finish-args:
  - --talk-name=io.github.XZram1
  - --system-talk-name=io.github.XZram1
```

## Snapshot limitations

- **Startup snapshots** require host `xzramd` or `pkexec` access to `xzram-helper`.
- **Restore** prompts for polkit authorization on the host.
- **Snapshot deletion** is not exposed in the GUI; use `xzram snapshot delete` on the host.

## Recommended setup

1. Install native `xzram` package and enable `xzramd.service`.
2. Install Flatpak `io.github.XZram` GUI (when published).
3. Use GUI for staging/review; rely on host daemon for snapshots and apply.
