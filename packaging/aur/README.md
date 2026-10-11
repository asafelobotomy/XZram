# AUR package (`xzram`)

Split package source for [AUR](https://aur.archlinux.org/): `xzram` (CLI/helper/daemon) and `xzram-gui` (Qt6).

| File | Role |
|------|------|
| `PKGBUILD` | AUR-ready build from GitHub tag `v$pkgver` |
| `.SRCINFO` | Generated metadata (`makepkg --printsrcinfo`) |
| `xzram.install` | Hint-only `post_install` (identical copy of repo-root `xzram.install`; never enables units) |

The repo-root `PKGBUILD` is for local `file://$startdir` experiments only — do not push it to the AUR.

## Build locally (no GitHub release required)

On Arch (or in an Arch container):

```bash
AUR_MODE=local AUR_OUT=$PWD/aur-out ./scripts/ci-aur-package.sh
```

## Release / pin checksums

1. Tag and publish a GitHub Release: `v$pkgver` (e.g. `v0.3.2`; must match `pkgver`).
2. Run release CI (automatic on `release: published`) or:

```bash
AUR_MODE=release AUR_TAG=v0.3.2 AUR_OUT=$PWD/aur-out ./scripts/ci-aur-package.sh
```

3. Copy `aur-out/PKGBUILD`, `aur-out/.SRCINFO` (checksums pinned), and `aur-out/xzram.install` into the AUR clone and push:

```bash
git clone ssh://aur@aur.archlinux.org/xzram.git
cp aur-out/PKGBUILD aur-out/.SRCINFO aur-out/xzram.install xzram/
cd xzram
git add PKGBUILD .SRCINFO xzram.install
git commit -m "Update to 0.3.2"
git push
```

Commit the pinned `sha256sums` (and regenerated `.SRCINFO`) back into `packaging/aur/` on the main branch after the
release build so the tree matches AUR. Until then `packaging/aur/PKGBUILD` carries an all-zero placeholder sum:
`make version-check` warns about it, and fails if `PKGBUILD`/`.SRCINFO` disagree on `pkgver`/`sha256sums` or if a
bumped `pkgver` reuses the checksum pinned for an earlier version.
