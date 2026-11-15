#!/usr/bin/env bash
# Remove xzram system install (requires root). Leaves /var/lib/xzram data intact.
set -euo pipefail

echo "==> Stopping xzramd"
systemctl stop xzramd.service 2>/dev/null || true
systemctl disable xzramd.service 2>/dev/null || true

echo "==> Removing system files"
make -C "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)" uninstall
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
