#!/usr/bin/env bash
# Fail if any source file exceeds MAX_LOC (default 400).
# Scope: Rust and Qt C++/headers under crates/ and gui/ (excludes generated build trees).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MAX_LOC="${MAX_LOC:-400}"
failed=0
count=0

while IFS= read -r -d '' file; do
  lines=$(wc -l < "$file")
  count=$((count + 1))
  if (( lines > MAX_LOC )); then
    printf 'LOC limit %s exceeded: %s has %s lines\n' "$MAX_LOC" "${file#"$ROOT"/}" "$lines" >&2
    failed=1
  fi
done < <(
  find "$ROOT/crates" "$ROOT/gui" -type f \( \
    -name '*.rs' -o -name '*.cpp' -o -name '*.h' -o -name '*.hpp' \
  \) ! -path '*/build-gui/*' ! -path '*/target/*' -print0 2>/dev/null
)

if (( failed )); then
  echo "loc-check failed (limit ${MAX_LOC} lines per file)" >&2
  exit 1
fi

echo "loc-check OK (${count} files, limit ${MAX_LOC})"
