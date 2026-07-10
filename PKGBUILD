# Maintainer: XZram contributors
pkgname=xzram
pkgver=0.1.0
pkgrel=1
pkgdesc="Cross-distro Linux swap management (zram, swap files, sysctl tuning)"
arch=('x86_64' 'aarch64')
url="https://github.com/xzram/xzram"
license=('GPL-3.0-or-later')
depends=('polkit' 'systemd' 'util-linux')
makedepends=('rust' 'cargo')
source=("$pkgname-$pkgver.tar.gz::file://$startdir/..")
sha256sums=('SKIP')

prepare() {
  cd "$pkgname-$pkgver"
  export CARGO_TARGET_DIR=target
}

build() {
  cd "$pkgname-$pkgver"
  cargo build --release --locked
}

package() {
  cd "$pkgname-$pkgver"
  install -Dm755 target/release/xzram "$pkgdir/usr/bin/xzram"
  install -Dm755 target/release/xzram-helper "$pkgdir/usr/libexec/xzram-helper"
  install -Dm644 data/io.github.xzram.policy "$pkgdir/usr/share/polkit-1/actions/io.github.xzram.policy"
  install -Dm644 data/bash-completion/xzram "$pkgdir/usr/share/bash-completion/completions/xzram"
  install -Dm644 docs/SCOPE.md "$pkgdir/usr/share/doc/xzram/SCOPE.md"
  install -Dm644 docs/GUI-PHASE2.md "$pkgdir/usr/share/doc/xzram/GUI-PHASE2.md"
}
