# AUR package (`xzram`)

Split package source for [AUR](https://aur.archlinux.org/): `xzram` (CLI/helper/daemon) and `xzram-gui` (Qt6).

| File | Role |
|------|------|
| `PKGBUILD` | AUR-ready build from GitHub tag `v$pkgver` |
| `.SRCINFO` | Generated metadata (`makepkg --printsrcinfo`) |

The repo-root `PKGBUILD` is for local `file://$startdir` experiments only — do not push it to the AUR.

## Build locally (no GitHub release required)

On Arch (or in an Arch container):

```bash
AUR_MODE=local AUR_OUT=$PWD/aur-out ./scripts/ci-aur-package.sh
```

## Release / pin checksums

1. Tag and publish a GitHub Release: `v0.3.0` (must match `pkgver`).
2. Run release CI (automatic on `release: published`) or:

```bash
AUR_MODE=release AUR_TAG=v0.3.0 AUR_OUT=$PWD/aur-out ./scripts/ci-aur-package.sh
```

3. Copy `aur-out/PKGBUILD` and `aur-out/.SRCINFO` (checksums pinned) into the AUR clone and push:

```bash
git clone ssh://aur@aur.archlinux.org/xzram.git
cp aur-out/PKGBUILD aur-out/.SRCINFO xzram/
cd xzram
git add PKGBUILD .SRCINFO
git commit -m "Update to 0.3.0"
git push
```

Commit the pinned `sha256sums` back into `packaging/aur/` on the main branch after the first successful release build so the tree matches AUR.
