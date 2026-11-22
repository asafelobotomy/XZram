Name:           xzram
Version:        0.7.5
Release:        1%{?dist}
Summary:        Cross-distro Linux swap management

License:        GPL-3.0-or-later
URL:            https://github.com/asafelobotomy/XZram
# GitHub tag archives extract to XZram-%%{version}/ (leading v stripped).
Source0:        %{url}/archive/refs/tags/v%{version}.tar.gz#/%{name}-%{version}.tar.gz

# rust-version in Cargo.toml
BuildRequires:  rust >= 1.89
BuildRequires:  cargo >= 1.89
BuildRequires:  cmake qt6-qtbase-devel systemd-rpm-macros
Requires:       polkit systemd util-linux
Recommends:     zram-generator
Suggests:       %{name}-gui
# Swapfiles on btrfs are made with `btrfs filesystem mkswapfile` when it is there
Suggests:       btrfs-progs

%description
XZram is a CLI and system D-Bus daemon for creating, removing, and customizing
swap on systemd-based Linux distributions. It supports zram via
systemd-zram-generator, swap file management, sysctl tuning, and configuration
snapshots.

%package gui
Summary:        Qt6 GUI for XZram swap management
Requires:       %{name} = %{version}-%{release} qt6-qtbase

%description gui
Desktop GUI for XZram that shells out to the xzram CLI for status, staging,
and polkit-gated apply.

%prep
%autosetup -n XZram-%{version}

%build
make build build-gui

%check
cargo test --release --locked

%install
# The Makefile's *-files targets are the one install list (shared with debian/rules).
make install-files DESTDIR=%{buildroot}

%files
%license LICENSE
%doc docs/SCOPE.md docs/SNAPSHOTS.md
%{_bindir}/xzram
%{_libexecdir}/xzram-helper
%{_libexecdir}/xzramd
%{_datadir}/polkit-1/actions/io.github.xzram.policy
%{_datadir}/bash-completion/completions/xzram
%{_unitdir}/xzramd.service
%{_datadir}/dbus-1/system.d/io.github.XZram.conf
%{_datadir}/dbus-1/system-services/io.github.XZram1.service
%{_datadir}/uq/settings/pages/xzram/
%{_datadir}/uq/diag/xzram.toml

%files gui
%doc docs/GUI-PHASE2.md
%{_bindir}/xzram-qt
%{_datadir}/applications/io.github.XZram.desktop
%{_datadir}/metainfo/io.github.XZram.metainfo.xml
%{_datadir}/icons/hicolor/*/apps/io.github.XZram.png

%post
%systemd_post xzramd.service

%preun
%systemd_preun xzramd.service

%postun
%systemd_postun xzramd.service

%changelog
* Sun Nov 22 2026 XZram contributors <xzram@example.com> - 0.7.5-1
- Problems show in uQ: uQ's log format, XZRAM-001..007 codes, crash records (uq-diag)

* Sat Nov 21 2026 XZram contributors <xzram@example.com> - 0.7.4-1
- GUI: follows the uQ theme through uQ's shared uq-theme module; one main desktop category
* Fri Nov 20 2026 XZram contributors <xzram@example.com> - 0.7.3-1
- GUI: no "System healthy" before or without a health check
* Fri Nov 20 2026 XZram contributors <xzram@example.com> - 0.7.2-1
- Update clap, clap_complete and zbus
* Tue Nov 17 2026 XZram contributors <xzram@example.com> - 0.7.1-1
- GUI follows the uQ theme and re-themes live
* Mon Nov 16 2026 XZram contributors <xzram@example.com> - 0.7.0-1
- uQ Settings page rebuilt; config show/set, --dry-run plans and --ndjson progress in the CLI
* Fri Nov 13 2026 XZram contributors <xzram@example.com> - 0.6.0-1
- Saturated zram, memory guard, drain-overflow, btrfs swapfile placement

* Thu Nov 12 2026 XZram contributors <xzram@example.com> - 0.5.0-1
- uQ Settings: a "Memory and swap" page (optional, needs uq)
- Snapshot restore no longer panics on non-UTF-8 snapshot paths
- Update zbus_polkit to 5.1.0 and tokio to 1.54.1 (security advisories)

* Wed Oct 14 2026 XZram contributors <xzram@example.com> - 0.4.3-1
- Doctor: past OOM kills are info with current advice once OOM protection is active
- GUI: dashboard swap table columns fit their contents
- Tests: fix an env race that failed CI intermittently

* Wed Oct 14 2026 XZram contributors <xzram@example.com> - 0.4.2-1
- Helper and xzramd reject unknown payload fields; CLI refuses helpers older than 0.4.2 before prompting
- xzram-helper --version
- Root PKGBUILD builds from git+file:// (local checkout package)

* Wed Oct 14 2026 XZram contributors <xzram@example.com> - 0.4.1-1
- systemd-oomd policy shields session.slice so apps are killed before the compositor and shell
- Recommended defaults: MGLRU min_ttl_ms=1000, THP defrag=defer+madvise, dirty writeback bytes, vfs_cache_pressure
- Doctor: oomd_session_unprotected, thp_always_low_ram

* Wed Oct 14 2026 XZram contributors <xzram@example.com> - 0.4.0-1
- Recommended defaults enable systemd-oomd with ManagedOOM kill policies; rollback undoes it
- Grow an undersized /swap/swapfile instead of reporting "already matches"
- Warn when disk swap is on a rotational drive
- Doctor: OOM kills, memory pressure (PSI), swap headroom, overflow size

* Sun Oct 11 2026 XZram contributors <xzram@example.com> - 0.3.2-1
- Security: harden swapfile path handling against symlink races; tighten xzramd sandbox and input validation
- Atomic writes for fstab and all state files; fstab entries matched exactly
- Snapshot restore merges fstab swap entries and removes swapfiles added since; failed apply rolls back
- Resize keeps the old swap on failure and falls back to in-place when space is tight
- CLI exit code 3 for unhealthy doctor; GUI privileged operations no longer block

* Wed Sep 23 2026 XZram contributors <xzram@example.com> - 0.3.1-1
- Fix zram mm_stat field mislabeled as compression streams
- Doctor warns when zram is near full and disk swap absorbs overflow
- Fix xzramd D-Bus startup registration race

* Tue Aug 11 2026 XZram contributors <xzram@example.com> - 0.3.0-1
- Linked auto-optimize, recommend size scales, async GUI linked optimize
- Split GUI subpackage; store.read polkit; audit follow-ups

* Wed Jul 22 2026 XZram contributors <xzram@example.com> - 0.2.0-1
- GUI CLI-first runner, settings/snapshot tabs, recommend hardening, versioning

* Fri Jul 10 2026 XZram contributors <xzram@example.com> - 0.1.0-1
- Bundle xzram-qt GUI and snapshot subsystem
