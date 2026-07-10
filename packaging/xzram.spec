Name:           xzram
Version:        0.1.0
Release:        1%{?dist}
Summary:        Cross-distro Linux swap management

License:        GPL-3.0-or-later
URL:            https://github.com/xzram/xzram
Source0:        %{name}-%{version}.tar.gz

BuildRequires:  rust cargo
Requires:       polkit systemd util-linux

%description
XZram is a CLI tool for creating, removing, and customizing swap on
systemd-based Linux distributions. It supports zram via systemd-zram-generator,
swap file management, and sysctl tuning.

%prep
%autosetup

%build
cargo build --release

%install
install -Dm755 target/release/xzram %{buildroot}%{_bindir}/xzram
install -Dm755 target/release/xzram-helper %{buildroot}%{_libexecdir}/xzram-helper
install -Dm644 data/io.github.xzram.policy %{buildroot}%{_datadir}/polkit-1/actions/io.github.xzram.policy
install -Dm644 data/bash-completion/xzram %{buildroot}%{_datadir}/bash-completion/completions/xzram

%files
%{_bindir}/xzram
%{_libexecdir}/xzram-helper
%{_datadir}/polkit-1/actions/io.github.xzram.policy
%{_datadir}/bash-completion/completions/xzram

%changelog
* Fri Jul 10 2026 XZram contributors <xzram@example.com> - 0.1.0-1
- Initial package
