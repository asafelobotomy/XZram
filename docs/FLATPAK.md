# XZram Flatpak Guide

## Distribution model

The Flatpak GUI cannot write `/etc` directly. Install the native host package first
(provides `xzramd`, `xzram-helper`, polkit policy, and snapshot storage).

When packages are published to distro repos (AUR, COPR, etc.):

```bash
# Arch (when packaged)
pacman -S xzram

# Fedora (when packaged)
dnf install xzram

# Debian/Ubuntu (when packaged)
apt install xzram
```

Until then, build and install from this repository:

- From source: `make install` / `make install-cli` (see README)
- Packaging sources: [`PKGBUILD`](../PKGBUILD), [`debian/`](../debian/),
  [`packaging/xzram.spec`](../packaging/xzram.spec)

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

1. Install native `xzram` and enable `xzramd.service`.
2. Install Flatpak `io.github.XZram` GUI (when published).
3. Use GUI for staging/review; rely on host daemon for snapshots and apply.
