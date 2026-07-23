#!/usr/bin/env bash
# Manual smoke checklist for xzramd polkit subject (not run in CI).
# Expect: unprivileged Apply prompts for admin or AccessDenied — never silent success as root.
set -euo pipefail
echo "1) As normal user: xzram --dbus apply   (should polkit-prompt or deny)"
echo "2) As normal user: xzram --dbus zram disable  (DisableZram then Apply; prompt/deny)"
echo "3) Confirm subject is system-bus-name (not peer uid of xzramd): journalctl -u xzramd -b"
echo "Done — perform steps interactively on a machine with polkit agent."
