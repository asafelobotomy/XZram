#!/usr/bin/env bash
# The uQ Settings page is installed by the Makefile and by both PKGBUILDs. Run each real install
# line into a temp root and check that exactly the files of uq/pages/xzram/ arrive, in
# usr/share/uq/settings/pages/xzram/. Nothing from tests/ can be shipped this way.
set -euo pipefail

cd "$(cd "$(dirname "$0")/.." && pwd)"
want="$(cd uq/pages/xzram && find . -type f | LC_ALL=C sort)"
[[ -n "$want" ]] || { echo "uq/pages/xzram is empty" >&2; exit 1; }
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

check() {
  local label="$1" root="$2" got
  got="$(cd "$root/usr/share/uq/settings/pages/xzram" && find . -type f | LC_ALL=C sort)"
  if [[ "$got" != "$want" ]]; then
    printf '%s installs the wrong files.\nwanted:\n%s\ngot:\n%s\n' "$label" "$want" "$got" >&2
    exit 1
  fi
  if [[ "$(cd "$root" && find . -type f | wc -l)" != "$(wc -l <<<"$want")" ]]; then
    echo "$label installs something besides the page" >&2
    exit 1
  fi
  echo "ok: $label"
}

# Makefile: the one line of install-cli that mentions the page (-o build skips the cargo build).
mk="$tmp/make"
make -s -n -o build install-cli DESTDIR="$mk" | grep 'uq/settings/pages/xzram' | bash
check "Makefile install-cli" "$mk"

# Both PKGBUILDs: the line in package_xzram, with $pkgdir pointed at a temp root.
for pb in PKGBUILD packaging/aur/PKGBUILD; do
  root="$tmp/$(echo "$pb" | tr / _)"
  line="$(sed -n '/^package_xzram()/,/^}/p' "$pb" | grep 'uq/settings/pages/xzram')"
  [[ "$(wc -l <<<"$line")" == 1 ]] || { echo "$pb: want one page install line in package_xzram" >&2; exit 1; }
  pkgdir="$root" bash -c "$line"
  check "$pb" "$root"
  grep -q "'uq: the Memory and swap page in uQ Settings'" "$pb" || { echo "$pb: missing the uq optdepends" >&2; exit 1; }
done
grep -q 'optdepends = uq: the Memory and swap page in uQ Settings' packaging/aur/.SRCINFO \
  || { echo "packaging/aur/.SRCINFO: missing the uq optdepends" >&2; exit 1; }
