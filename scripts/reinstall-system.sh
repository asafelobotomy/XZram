#!/usr/bin/env bash
# Uninstall and reinstall xzram system components (requires root).
# Build first as your user: make build build-gui
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# Check before removing anything, so a missing build doesn't leave the system uninstalled.
for bin in target/release/xzram target/release/xzram-helper target/release/xzramd \
  build-gui/xzram-qt/xzram-qt; do
  [[ -x "$bin" ]] || { echo "Missing $bin; run 'make build build-gui' as your user first" >&2; exit 1; }
done

echo "==> Stopping xzramd"
systemctl stop xzramd.service 2>/dev/null || true
systemctl disable xzramd.service 2>/dev/null || true

echo "==> Removing old system files"
rm -f /usr/bin/xzram /usr/bin/xzram-qt
rm -f /usr/libexec/xzram-helper /usr/libexec/xzramd
rm -f /usr/lib/systemd/system/xzramd.service
rm -f /usr/share/dbus-1/system.d/io.github.XZram.conf
rm -f /usr/share/dbus-1/system-services/io.github.XZram1.service
rm -f /usr/share/polkit-1/actions/io.github.xzram.policy
rm -f /usr/share/bash-completion/completions/xzram
rm -rf /usr/share/uq/settings/pages/xzram
rm -f /usr/share/applications/io.github.XZram.desktop
rm -f /usr/share/metainfo/io.github.XZram.metainfo.xml
rm -f /usr/share/icons/hicolor/*/apps/io.github.XZram.png

echo "==> Installing (CLI, daemon, GUI) from the existing build"
# Build as your user first ('make build build-gui'); building here would run cargo and
# cmake as root and leave target/ and build-gui/ root-owned.
make install-files
make install-post

if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  gtk-update-icon-cache -f /usr/share/icons/hicolor 2>/dev/null || true
fi

systemctl daemon-reload
echo "==> Done. Installed binaries:"
command -v xzram && xzram --version 2>/dev/null || true
command -v xzram-qt || echo "  WARNING: xzram-qt not in PATH"
echo "==> xzramd status:"
systemctl status xzramd.service --no-pager || true
