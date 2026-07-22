# Agent notes for XZram

CLI-first Linux swap tooling (Rust workspace + optional Qt6 GUI).

## Bootstrap (cold start)

1. Prerequisites: Rust ≥ 1.75, Cargo, Linux/systemd. GUI needs cmake + Qt6.
2. Fast path (no sudo):

```bash
cargo run -p xzram-cli -- status
# or after release build:
./target/release/xzram detect
./target/release/xzram doctor
```

3. Do **not** use `sudo` for `status` / `detect` / `doctor` smoke checks.
4. Privileged paths (`apply`, `daemon start`, write helpers) need polkit/root — treat
   auth failures as expected unless you intentionally test apply.

## Surfaces

| Surface | How | Notes |
|---------|-----|--------|
| CLI | `cargo run -p xzram-cli -- …` or `./target/release/xzram` | Primary |
| Helper | `xzram-helper` via pkexec | Root only |
| Daemon | `xzramd` / `systemctl enable --now xzramd` | Privileged next step |
| GUI | `make build-gui` → `build-gui/xzram-qt/xzram-qt` | Optional; Qt6; CLI-backed; auto-refresh; `XZRAM_CLI` override |

Prefer `make install-cli` over `make install` when Qt is not needed.

## Verify a small change

```bash
make check          # cargo check -p xzram
make test-lib       # cargo test -p xzram --lib (~seconds warm)
make lint           # fmt --check + clippy -D warnings + loc-check (≤400 lines/file; matches CI)
make gui-smoke      # only for GUI edits
```

`xzramd` is a binary crate (`cargo test -p xzramd --lib` is not the right gate).
CLI/helper smoke: `cargo test -p xzram-cli` and `cargo test -p xzram-helper`.
GUI gate: `make gui-smoke` (not QTest).
Optional: `pre-commit install` runs fmt-check, clippy, and `cargo test -p xzram --lib` on commit.
Debug logs: `RUST_LOG=debug cargo run -p xzram-cli -- status` (see [docs/DEV-ENV.md](docs/DEV-ENV.md)).
Operator health surface: `xzram doctor` / `xzram status` (no root).
See README **Verify** and [docs/DEV-ENV.md](docs/DEV-ENV.md) for `XZRAM_*` overrides.

## Docs map

- [README.md](README.md) — user quick start, install, CLI
- [docs/SCOPE.md](docs/SCOPE.md) — in/out of scope
- [docs/GUI-PHASE2.md](docs/GUI-PHASE2.md) — GUI is CLI-backed; daemon optional for other clients/Flatpak
- [docs/RECOMMENDATIONS.md](docs/RECOMMENDATIONS.md) — defaults profiles
- [docs/SNAPSHOTS.md](docs/SNAPSHOTS.md) — snapshot semantics
