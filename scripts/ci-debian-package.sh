#!/usr/bin/env bash
# Debian package smoke for CI: Build-Depends from apt, Rust from rustup, then dpkg-buildpackage.
# debian/control asks for rustc/cargo >= 1.89 (Cargo.toml rust-version); trixie's are older, so
# those two come from rustup and are the only unmet Build-Depends this script lets through.
# Expects the repo mounted read-only at /src; builds from a writable copy.
set -euo pipefail

export DEBIAN_FRONTEND=noninteractive
export CARGO_HOME="${CARGO_HOME:-/tmp/cargo-home}"
export RUSTUP_HOME="${RUSTUP_HOME:-/tmp/rustup-home}"
mkdir -p "$CARGO_HOME" "$RUSTUP_HOME"

apt-get update
apt-get install -y \
  curl ca-certificates \
  debhelper cmake qt6-base-dev \
  pkg-config libssl-dev \
  build-essential

curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
  | sh -s -- -y --default-toolchain stable --profile minimal
# shellcheck source=/dev/null
. "$CARGO_HOME/env"
export PATH="$CARGO_HOME/bin:$PATH"

WORKDIR=/tmp/xzram-debian
rm -rf "$WORKDIR"
cp -a /src "$WORKDIR"
cd "$WORKDIR"
unmet="$(dpkg-checkbuilddeps 2>&1 | sed -n 's/.*Unmet build dependencies: //p' || true)"
other="$(sed -E 's/(rustc|cargo) \(>= [0-9.]+\)//g; s/[[:space:]]//g' <<<"$unmet")"
[[ -z "$other" ]] || { echo "Unmet build dependencies: $unmet" >&2; exit 1; }
rustc --version
# debian/rules sets CARGO_TARGET_DIR=$(CURDIR)/target (writable copy). -d: rustc/cargo are
# from rustup (checked above), so dpkg's own Build-Depends check would refuse them.
dpkg-buildpackage -us -uc -b -d
ls -la /tmp/*.deb "$WORKDIR"/../*.deb 2>/dev/null || ls -la ../*.deb
