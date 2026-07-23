#!/usr/bin/env bash
# Distro smoke helper for CI containers.
# Expects source at /src (read-only OK). Writes Cargo output under /tmp.
set -euo pipefail

DISTRO="${1:?usage: ci-distro-smoke.sh <fedora|ubuntu|debian|arch>}"

export CARGO_HOME="${CARGO_HOME:-/tmp/cargo-home}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-/tmp/xzram-target}"
mkdir -p "$CARGO_HOME" "$CARGO_TARGET_DIR"

install_rustup() {
  if ! command -v curl >/dev/null 2>&1; then
    echo "curl required to install rustup" >&2
    exit 1
  fi
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- -y --default-toolchain stable --profile minimal
  # shellcheck source=/dev/null
  . "$HOME/.cargo/env"
  rustc --version
  cargo --version
}

case "$DISTRO" in
  fedora)
    dnf install -y curl ca-certificates polkit systemd util-linux gcc make pkgconf-pkg-config openssl-devel
    ;;
  ubuntu|debian)
    export DEBIAN_FRONTEND=noninteractive
    apt-get update
    apt-get install -y curl ca-certificates polkitd systemd util-linux \
      build-essential pkg-config libssl-dev
    ;;
  arch)
    pacman -Syu --noconfirm curl ca-certificates polkit systemd util-linux \
      base-devel pkgconf openssl
    ;;
  *)
    echo "unknown distro: $DISTRO" >&2
    exit 1
    ;;
esac

install_rustup
cd /src
cargo build --release
cargo test
