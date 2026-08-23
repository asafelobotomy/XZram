#!/usr/bin/env bash
# Build Arch packages from packaging/aur in an Arch container (or native Arch).
#
# Modes:
#   local   (default) — git-archive a source tarball matching GitHub layout
#   release — download the GitHub tag archive, pin sha256sums, then makepkg
#
# Env:
#   AUR_MODE=local|release
#   AUR_TAG=v0.3.0          (release mode; default: v$pkgver from PKGBUILD)
#   AUR_OUT=/path/to/outdir (default: /tmp/xzram-aur-out)
#   SRC_ROOT=/path/to/repo  (default: auto-detect from script location)
set -euo pipefail

AUR_MODE="${AUR_MODE:-local}"
AUR_OUT="${AUR_OUT:-/tmp/xzram-aur-out}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SRC_ROOT="${SRC_ROOT:-$(cd "$SCRIPT_DIR/.." && pwd)}"
AUR_DIR="$SRC_ROOT/packaging/aur"

if [[ ! -f "$AUR_DIR/PKGBUILD" ]]; then
  echo "missing $AUR_DIR/PKGBUILD" >&2
  exit 1
fi

pkgver="$(sed -n 's/^pkgver=//p' "$AUR_DIR/PKGBUILD" | head -1)"
if [[ -z "$pkgver" ]]; then
  echo "could not read pkgver from PKGBUILD" >&2
  exit 1
fi

AUR_TAG="${AUR_TAG:-v${pkgver}}"
if [[ "$AUR_MODE" == "release" && "$AUR_TAG" != "v${pkgver}" ]]; then
  echo "AUR_TAG=$AUR_TAG does not match packaging/aur pkgver=v${pkgver}" >&2
  echo "Bump packaging/aur/PKGBUILD pkgver before cutting the release." >&2
  exit 1
fi
_srcname=XZram
tarball="${_srcname}-${pkgver}.tar.gz"
# GitHub tag archives extract to XZram-$pkgver (leading v on the tag is stripped).
extract_prefix="${_srcname}-${pkgver}"
source_url="https://github.com/asafelobotomy/XZram/archive/refs/tags/${AUR_TAG}.tar.gz"

workdir="$(mktemp -d /tmp/xzram-aur-build.XXXXXX)"
cleanup() { rm -rf "$workdir"; }
trap cleanup EXIT

cp -a "$AUR_DIR/." "$workdir/"
cd "$workdir"

case "$AUR_MODE" in
  local)
    if ! command -v git >/dev/null 2>&1; then
      echo "git required for AUR_MODE=local" >&2
      exit 1
    fi
    git -C "$SRC_ROOT" archive \
      --format=tar.gz \
      --prefix="${extract_prefix}/" \
      -o "$workdir/$tarball" \
      HEAD
    sum="$(sha256sum "$tarball" | awk '{print $1}')"
    # Local file source (same extract dir as a GitHub v$pkgver tag archive).
    sed -i \
      -e "s|^source=.*|source=(\"$tarball\")|" \
      -e "s|^sha256sums=.*|sha256sums=('$sum')|" \
      PKGBUILD
    ;;
  release)
    echo "Downloading $source_url"
    curl -fsSL -o "$tarball" "$source_url"
    top="$(tar -tzf "$tarball" | awk -F/ 'NR==1 { print $1; exit }')"
    if [[ "$top" != "$extract_prefix" ]]; then
      echo "unexpected archive root '$top' (expected '$extract_prefix')" >&2
      exit 1
    fi
    sum="$(sha256sum "$tarball" | awk '{print $1}')"
    # AUR-publishable source line + pinned checksum (makepkg reuses local $tarball).
    sed -i \
      -e 's|^source=.*|source=("$_srcname-$pkgver.tar.gz::https://github.com/asafelobotomy/XZram/archive/refs/tags/v$pkgver.tar.gz")|' \
      -e "s|^sha256sums=.*|sha256sums=('$sum')|" \
      PKGBUILD
    ;;
  *)
    echo "unknown AUR_MODE=$AUR_MODE (use local|release)" >&2
    exit 1
    ;;
esac

# Non-root builder required by makepkg.
if [[ "$(id -u)" -eq 0 ]]; then
  if ! id builder >/dev/null 2>&1; then
    useradd --create-home --shell /bin/bash builder
  fi
  printf 'builder ALL=(ALL) NOPASSWD: /usr/bin/pacman\n' >/etc/sudoers.d/builder-pacman
  chmod 440 /etc/sudoers.d/builder-pacman
  chown -R builder:builder "$workdir"
  run() { runuser -u builder -- env HOME=/home/builder "$@"; }
else
  run() { env "$@"; }
fi

# Ensure build tools (best-effort when root in container).
if [[ "$(id -u)" -eq 0 ]]; then
  pacman -Syu --noconfirm --needed \
    base-devel git namcap sudo curl \
    rust cargo cmake qt6-base \
    polkit systemd util-linux
fi

run makepkg --syncdeps --cleanbuild --noconfirm --force
run bash -c 'makepkg --printsrcinfo > .SRCINFO'

if command -v namcap >/dev/null 2>&1; then
  namcap PKGBUILD || true
  shopt -s nullglob
  pkgs=(./*.pkg.tar.zst ./*.pkg.tar.xz)
  if ((${#pkgs[@]})); then
    namcap "${pkgs[@]}" || true
  fi
fi

mkdir -p "$AUR_OUT"
cp -a PKGBUILD .SRCINFO "$AUR_OUT/"
shopt -s nullglob
built=(./*.pkg.tar.zst ./*.pkg.tar.xz)
if ((${#built[@]} == 0)); then
  echo "makepkg produced no package archives" >&2
  exit 1
fi
cp -a "${built[@]}" "$AUR_OUT/"
ls -la "$AUR_OUT"
echo "AUR packages written to $AUR_OUT"
