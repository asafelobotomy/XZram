#!/usr/bin/env bash
# Distro smoke helper for CI containers.
# Expects source at /src (read-only OK). Writes Cargo output under /tmp.
set -euo pipefail

DISTRO="${1:?usage: ci-distro-smoke.sh <fedora|ubuntu|debian|arch>}"

export CARGO_HOME="${CARGO_HOME:-/tmp/cargo-home}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-/tmp/xzram-target}"
export RUSTUP_HOME="${RUSTUP_HOME:-/tmp/rustup-home}"
mkdir -p "$CARGO_HOME" "$CARGO_TARGET_DIR" "$RUSTUP_HOME"

install_rustup() {
  if ! command -v curl >/dev/null 2>&1; then
    echo "curl required to install rustup" >&2
    exit 1
  fi
  local attempt
  for attempt in 1 2 3; do
    if curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
      | sh -s -- -y --default-toolchain stable --profile minimal; then
      break
    fi
    if [[ "$attempt" -eq 3 ]]; then
      echo "rustup install failed after retries" >&2
      exit 1
    fi
    echo "rustup install failed (attempt $attempt); retrying..." >&2
    sleep $((attempt * 5))
  done
  # With CARGO_HOME set, the env file lives under CARGO_HOME (not ~/.cargo).
  # shellcheck source=/dev/null
  . "$CARGO_HOME/env"
  export PATH="$CARGO_HOME/bin:$PATH"
  rustc --version
  cargo --version
}

case "$DISTRO" in
  fedora)
    dnf install -y curl ca-certificates polkit systemd util-linux \
      gcc make pkgconf-pkg-config openssl-devel
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
