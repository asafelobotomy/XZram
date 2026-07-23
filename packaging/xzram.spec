Name:           xzram
Version:        0.2.0
Release:        1%{?dist}
Summary:        Cross-distro Linux swap management

License:        GPL-3.0-or-later
URL:            https://github.com/xzram/xzram
Source0:        %{name}-%{version}.tar.gz

BuildRequires:  rust cargo cmake qt6-qtbase-devel
Requires:       polkit systemd util-linux qt6-qtbase

%description
XZram is a CLI and Qt6 GUI for creating, removing, and customizing swap on
systemd-based Linux distributions. It supports zram via systemd-zram-generator,
swap file management, sysctl tuning, and configuration snapshots.

%prep
%autosetup

%build
cargo build --release
cmake -S gui -B build-gui
cmake --build build-gui

%install
install -Dm755 target/release/xzram %{buildroot}%{_bindir}/xzram
install -Dm755 target/release/xzram-helper %{buildroot}%{_libexecdir}/xzram-helper
install -Dm755 target/release/xzramd %{buildroot}%{_libexecdir}/xzramd
install -Dm755 build-gui/xzram-qt/xzram-qt %{buildroot}%{_bindir}/xzram-qt
install -Dm644 data/io.github.xzram.policy %{buildroot}%{_datadir}/polkit-1/actions/io.github.xzram.policy
install -Dm644 data/bash-completion/xzram %{buildroot}%{_datadir}/bash-completion/completions/xzram
install -Dm644 data/io.github.XZram.service %{buildroot}%{_unitdir}/xzramd.service
install -Dm644 data/io.github.XZram.conf %{buildroot}%{_datadir}/dbus-1/system.d/io.github.XZram.conf
install -Dm644 data/dbus-1/system-services/io.github.XZram1.service %{buildroot}%{_datadir}/dbus-1/system-services/io.github.XZram1.service
install -Dm644 data/io.github.XZram.desktop %{buildroot}%{_datadir}/applications/io.github.XZram.desktop
install -Dm644 data/io.github.XZram.metainfo.xml %{buildroot}%{_datadir}/metainfo/io.github.XZram.metainfo.xml
for size in 32x32 48x48 64x64 128x128 256x256 512x512; do
  install -Dm644 data/icons/hicolor/${size}/apps/io.github.XZram.png \
    %{buildroot}%{_datadir}/icons/hicolor/${size}/apps/io.github.XZram.png
done

%files
%{_bindir}/xzram
%{_bindir}/xzram-qt
%{_libexecdir}/xzram-helper
%{_libexecdir}/xzramd
%{_datadir}/polkit-1/actions/io.github.xzram.policy
%{_datadir}/bash-completion/completions/xzram
%{_unitdir}/xzramd.service
%{_datadir}/dbus-1/system.d/io.github.XZram.conf
%{_datadir}/dbus-1/system-services/io.github.XZram1.service
%{_datadir}/applications/io.github.XZram.desktop
%{_datadir}/metainfo/io.github.XZram.metainfo.xml
%{_datadir}/icons/hicolor/*/apps/io.github.XZram.png

%post
%systemd_post xzramd.service

%postun
%systemd_postun_with_restart xzramd.service

%changelog
* Wed Jul 22 2026 XZram contributors <xzram@example.com> - 0.2.0-1
- GUI CLI-first runner, settings/snapshot tabs, recommend hardening, versioning

* Mon Jul 13 2026 XZram contributors <xzram@example.com> - 0.1.0-1
- Bundle xzram-qt GUI and snapshot subsystem
