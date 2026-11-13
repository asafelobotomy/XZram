# Agent notes for XZram

CLI-first Linux swap tooling (Rust workspace + optional Qt6 GUI).

## Bootstrap (cold start)

1. Prerequisites: Rust ≥ 1.89, Cargo, Linux/systemd. GUI needs cmake + Qt6.
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
| uQ Settings page | `uq/pages/xzram/` (`page.toml`, `Page.qml`, `logic.js`, components) | Reads and writes through `xzramd` over D-Bus (`GetStatus`, `GetZramConfig`, `GetSysctl`, `ListSwaps`, `ApplyNowZram`, `ApplyNowSysctl`); no `[[command]]`. Keep the method list in `page.toml` and `tests/uq/logic.test.mjs` in step with `crates/xzramd/src/dbus/`. See [uq/README.md](uq/README.md) |
| GUI | `make build-gui` → `build-gui/xzram-qt/xzram-qt` | Optional; Qt6; CLI-backed; auto-refresh; `XZRAM_ALLOW_DEV_CLI=1` + `XZRAM_CLI` override only with `-DXZRAM_DEV_OVERRIDES=ON` (see docs/DEV-ENV.md) |

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
uQ page (no Qt needed for the first two):

```bash
node --test 'tests/uq/*.test.mjs'     # logic.js and page.toml
./scripts/check-uq-install.sh         # Makefile and both PKGBUILDs install exactly the page folder
uqctl integrations check --no-programs uq/
```

`tests/uq/tst_page.qml` runs in CI (`uq` job) with `qmltestrunner`; the page needs a Qt 6 with QtQuick Controls to try by hand.

Optional: `pre-commit install` runs fmt-check, clippy, and `cargo test -p xzram --lib` on commit.
Debug logs: `RUST_LOG=debug cargo run -p xzram-cli -- status` (see [docs/DEV-ENV.md](docs/DEV-ENV.md)).
Operator health surface: `xzram doctor` / `xzram status` (no root). CLI exit codes: 0 ok, 1 error, 2 usage, 3 doctor unhealthy.
See README **Verify** and [docs/DEV-ENV.md](docs/DEV-ENV.md) for `XZRAM_*` overrides.

## Docs map

- [README.md](README.md) — user quick start, install, CLI
- [docs/SCOPE.md](docs/SCOPE.md) — in/out of scope
- [uq/README.md](uq/README.md) — the Memory and swap page for uQ Settings
- [docs/GUI-PHASE2.md](docs/GUI-PHASE2.md) — GUI is CLI-backed; daemon optional for other clients
- [docs/RECOMMENDATIONS.md](docs/RECOMMENDATIONS.md) — defaults profiles
- [docs/SNAPSHOTS.md](docs/SNAPSHOTS.md) — snapshot semantics
