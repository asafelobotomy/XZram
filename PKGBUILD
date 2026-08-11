# Maintainer: XZram contributors
pkgbase=xzram
pkgname=('xzram' 'xzram-gui')
pkgver=0.3.0
pkgrel=1
pkgdesc="Cross-distro Linux swap management (zram, swap files, sysctl tuning)"
arch=('x86_64' 'aarch64')
url="https://github.com/asafelobotomy/XZram"
license=('GPL-3.0-or-later')
makedepends=('rust' 'cargo' 'cmake' 'qt6-base')
source=("$pkgbase-$pkgver::file://$startdir")
sha256sums=('SKIP')

prepare() {
  cd "$pkgbase-$pkgver"
  export CARGO_TARGET_DIR=target
}

build() {
  cd "$pkgbase-$pkgver"
  cargo build --release --locked
  cmake -S gui -B build-gui
  cmake --build build-gui
}

package_xzram() {
  depends=('polkit' 'systemd' 'util-linux')
  optdepends=(
    'systemd-zram-generator: zram swap via systemd-zram-generator'
    'xzram-gui: Qt6 desktop GUI'
  )
  pkgdesc="CLI, helper, and D-Bus daemon for Linux swap management"

  cd "$pkgbase-$pkgver"
  install -Dm755 target/release/xzram "$pkgdir/usr/bin/xzram"
  install -Dm755 target/release/xzram-helper "$pkgdir/usr/libexec/xzram-helper"
  install -Dm755 target/release/xzramd "$pkgdir/usr/libexec/xzramd"
  install -Dm644 data/io.github.xzram.policy "$pkgdir/usr/share/polkit-1/actions/io.github.xzram.policy"
  install -Dm644 data/bash-completion/xzram "$pkgdir/usr/share/bash-completion/completions/xzram"
  install -Dm644 data/io.github.XZram.service "$pkgdir/usr/lib/systemd/system/xzramd.service"
  install -Dm644 data/io.github.XZram.conf "$pkgdir/usr/share/dbus-1/system.d/io.github.XZram.conf"
  install -Dm644 data/dbus-1/system-services/io.github.XZram1.service "$pkgdir/usr/share/dbus-1/system-services/io.github.XZram1.service"
  install -Dm644 docs/SCOPE.md "$pkgdir/usr/share/doc/xzram/SCOPE.md"
  install -Dm644 docs/SNAPSHOTS.md "$pkgdir/usr/share/doc/xzram/SNAPSHOTS.md"
}

package_xzram-gui() {
  depends=('xzram' 'qt6-base')
  pkgdesc="Qt6 GUI for XZram swap management"

  cd "$pkgbase-$pkgver"
  install -Dm755 build-gui/xzram-qt/xzram-qt "$pkgdir/usr/bin/xzram-qt"
  install -Dm644 data/io.github.XZram.desktop "$pkgdir/usr/share/applications/io.github.XZram.desktop"
  install -Dm644 data/io.github.XZram.metainfo.xml "$pkgdir/usr/share/metainfo/io.github.XZram.metainfo.xml"
  for size in 32x32 48x48 64x64 128x128 256x256 512x512; do
    install -Dm644 "data/icons/hicolor/${size}/apps/io.github.XZram.png" \
      "$pkgdir/usr/share/icons/hicolor/${size}/apps/io.github.XZram.png"
  done
  install -Dm644 docs/GUI-PHASE2.md "$pkgdir/usr/share/doc/xzram-gui/GUI-PHASE2.md"
}
