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
make uninstall

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
