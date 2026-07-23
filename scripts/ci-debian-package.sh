#!/usr/bin/env bash
# Debian package smoke for CI: rustup + full Build-Depends, then dpkg-buildpackage.
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
  build-essential \
  cargo rustc

curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
  | sh -s -- -y --default-toolchain stable --profile minimal
# shellcheck source=/dev/null
. "$CARGO_HOME/env"
export PATH="$CARGO_HOME/bin:$PATH"

cd /src
dpkg-checkbuilddeps
# debian/rules sets CARGO_TARGET_DIR=$(CURDIR)/target (writable mount).
dpkg-buildpackage -us -uc -b
ls -la ../*.deb
