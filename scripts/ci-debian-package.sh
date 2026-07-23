#!/usr/bin/env bash
# Debian package smoke for CI: rustup + full Build-Depends, then dpkg-buildpackage.
set -euo pipefail

export DEBIAN_FRONTEND=noninteractive

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
. "$HOME/.cargo/env"
export PATH="$HOME/.cargo/bin:$PATH"

cd /src
dpkg-checkbuilddeps
# debian/rules sets CARGO_TARGET_DIR=$(CURDIR)/target (writable mount).
dpkg-buildpackage -us -uc -b
ls -la ../*.deb
