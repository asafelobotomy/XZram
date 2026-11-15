#!/usr/bin/env bash
# RPM package smoke for CI: build packaging/xzram.spec (with %check) from a tarball of the
# checkout, the way a Fedora build would from the release tarball.
# Expects the repo mounted read-only at /src.
set -euo pipefail

dnf install -y rpm-build rpmdevtools git-core 'dnf-command(builddep)'
spec=/src/packaging/xzram.spec
dnf builddep -y "$spec"
rustc --version

version="$(tr -d '[:space:]' </src/VERSION)"
rpmdev-setuptree
git config --global --add safe.directory /src
# GitHub tag archives unpack to XZram-<version>/; match that layout.
git -C /src archive --format=tar.gz --prefix="XZram-${version}/" \
  -o "$HOME/rpmbuild/SOURCES/xzram-${version}.tar.gz" HEAD
rpmbuild -bb "$spec"

rpms=("$HOME"/rpmbuild/RPMS/*/xzram-*.rpm)
ls -la "${rpms[@]}"
main="$(ls "$HOME"/rpmbuild/RPMS/*/xzram-"${version}"-*.rpm)"
rpm -qlp "$main" | grep -qx /usr/share/uq/settings/pages/xzram/page.toml \
  || { echo "xzram rpm is missing the uQ page" >&2; exit 1; }
rpm -qlp "$main" | grep -qx /usr/libexec/xzram-helper \
  || { echo "xzram rpm is missing xzram-helper" >&2; exit 1; }
