#!/usr/bin/env bash
# Verify that VERSION matches every build/packaging surface, and that the AUR
# PKGBUILD and its .SRCINFO agree with each other (pkgver and sha256sums).
#
# Usage: scripts/version-check.sh   (run from anywhere; uses repo root)
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

fail=0
err() { echo "version-check: ERROR: $*" >&2; fail=1; }
warn() { echo "version-check: warning: $*" >&2; }

version="$(tr -d '[:space:]' <VERSION)"
[[ -n "$version" ]] || { echo "version-check: VERSION is empty" >&2; exit 1; }

check() { # check <file> <grep -E pattern> <description>
  grep -Eq -- "$2" "$1" || err "$1: $3 does not match VERSION=$version"
}

re_ver="${version//./\\.}"
check Cargo.toml "^version = \"${re_ver}\"" "workspace version"
check gui/CMakeLists.txt "project\\(xzram-qt VERSION ${re_ver}([^0-9.]|\$)" "CMake project version"
check PKGBUILD "^pkgver=${re_ver}\$" "pkgver"
check packaging/aur/PKGBUILD "^pkgver=${re_ver}\$" "pkgver"
check packaging/aur/.SRCINFO "^[[:space:]]*pkgver = ${re_ver}\$" "pkgver"
check packaging/xzram.spec "^Version:[[:space:]]*${re_ver}\$" "Version"
check data/io.github.XZram.metainfo.xml "version=\"${re_ver}\"" "release version"
head -1 debian/changelog | grep -Eq "^xzram \\(${re_ver}-[0-9]+\\)" \
  || err "debian/changelog: top entry does not match VERSION=$version"

# RPM %changelog top entry must be this version.
spec_top="$(sed -n '/^%changelog/,$p' packaging/xzram.spec | grep -m1 '^\*' || true)"
[[ "$spec_top" == *" - ${version}-"* ]] \
  || err "packaging/xzram.spec: top %changelog entry is not ${version}: ${spec_top}"

# --- AUR PKGBUILD <-> .SRCINFO consistency ---------------------------------
aur_pkgbuild=packaging/aur/PKGBUILD
aur_srcinfo=packaging/aur/.SRCINFO

pkgbuild_sums() { # print sha256sums entries from a PKGBUILD on stdin
  sed -n "/^sha256sums=(/,/)/p" | grep -Eo "'[^']*'|\"[^\"]*\"" | tr -d "'\""
}
pb_pkgver="$(sed -n 's/^pkgver=//p' "$aur_pkgbuild" | head -1)"
si_pkgver="$(sed -n 's/^[[:space:]]*pkgver = //p' "$aur_srcinfo" | head -1)"
[[ "$pb_pkgver" == "$si_pkgver" ]] \
  || err "AUR pkgver mismatch: PKGBUILD=$pb_pkgver .SRCINFO=$si_pkgver (run makepkg --printsrcinfo)"

pb_sums="$(pkgbuild_sums <"$aur_pkgbuild")"
si_sums="$(sed -n 's/^[[:space:]]*sha256sums = //p' "$aur_srcinfo")"
[[ -n "$pb_sums" ]] || err "$aur_pkgbuild: no sha256sums found"
[[ "$pb_sums" == "$si_sums" ]] \
  || err "AUR sha256sums mismatch between PKGBUILD and .SRCINFO (run makepkg --printsrcinfo)"
if grep -qx 'SKIP' <<<"$pb_sums"; then
  err "$aur_pkgbuild: sha256sums must not be SKIP for the AUR"
fi

for f in xzram.install packaging/aur/xzram.install; do
  [[ -f "$f" ]] || err "missing $f"
done
if [[ -f xzram.install && -f packaging/aur/xzram.install ]]; then
  cmp -s xzram.install packaging/aur/xzram.install \
    || err "xzram.install and packaging/aur/xzram.install differ"
fi
grep -q '^[[:space:]]*install = xzram.install$' "$aur_srcinfo" \
  || err "$aur_srcinfo: missing 'install = xzram.install'"

placeholder='0000000000000000000000000000000000000000000000000000000000000000'
if grep -qx "$placeholder" <<<"$pb_sums"; then
  warn "$aur_pkgbuild: sha256sums is the all-zero placeholder (tag v$pb_pkgver not pinned yet;"
  warn "  run AUR_MODE=release scripts/ci-aur-package.sh after tagging, then commit the sum)"
fi

# A pkgver bump must come with a new checksum: the current sum must not equal a
# sum previously pinned for a *different* pkgver in git history.
if command -v git >/dev/null 2>&1 && git rev-parse --git-dir >/dev/null 2>&1; then
  while read -r rev; do
    old="$(git show "$rev:$aur_pkgbuild" 2>/dev/null)" || continue
    old_ver="$(sed -n 's/^pkgver=//p' <<<"$old" | head -1)"
    [[ -n "$old_ver" && "$old_ver" != "$pb_pkgver" ]] || continue
    old_sums="$(pkgbuild_sums <<<"$old")"
    if [[ -n "$old_sums" && "$old_sums" == "$pb_sums" && "$pb_sums" != "$placeholder" ]]; then
      err "$aur_pkgbuild: sha256sums for $pb_pkgver equal the sum pinned for $old_ver (commit ${rev:0:7}); re-pin after bumping pkgver"
      break
    fi
  done < <(git log --format=%H -- "$aur_pkgbuild")
fi

if ((fail)); then
  exit 1
fi
echo "version $version OK"
