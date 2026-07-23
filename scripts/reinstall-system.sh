#!/usr/bin/env bash
# Uninstall and reinstall xzram system components (requires root).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

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
rm -f /usr/share/applications/io.github.XZram.desktop
rm -f /usr/share/metainfo/io.github.XZram.metainfo.xml
rm -f /usr/share/icons/hicolor/*/apps/io.github.XZram.png

echo "==> Building and installing (CLI, daemon, GUI)"
make install
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
