#!/usr/bin/env bash
# Remove xzram system install (requires root). Leaves /var/lib/xzram data intact.
set -euo pipefail

echo "==> Stopping xzramd"
systemctl stop xzramd.service 2>/dev/null || true
systemctl disable xzramd.service 2>/dev/null || true

echo "==> Removing system files"
rm -f /usr/bin/xzram /usr/bin/xzram-qt
rm -f /usr/libexec/xzram-helper /usr/libexec/xzramd
rm -f /usr/lib/systemd/system/xzramd.service
rm -f /usr/share/dbus-1/system.d/io.github.XZram.conf
rm -f /usr/share/dbus-1/system-services/io.github.XZram1.service
rm -f /usr/share/polkit-1/actions/io.github.xzram.policy
rm -f /usr/share/bash-completion/completions/xzram
rm -f /usr/share/applications/io.github.XZram.desktop
rm -f /usr/share/metainfo/io.github.XZram.metainfo.xml
rm -f /usr/share/icons/hicolor/*/apps/io.github.XZram.png
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  gtk-update-icon-cache -f /usr/share/icons/hicolor 2>/dev/null || true
fi

systemctl daemon-reload
busctl call org.freedesktop.DBus /org/freedesktop/DBus org.freedesktop.DBus ReloadConfig 2>/dev/null || true

echo "==> Done. Remaining xzram binaries (should be none):"
command -v xzram || echo "  xzram: not found"
command -v xzram-qt || echo "  xzram-qt: not found"
systemctl is-active xzramd.service 2>&1 || true
echo "Data dir preserved at /var/lib/xzram (remove manually to purge snapshots)"
