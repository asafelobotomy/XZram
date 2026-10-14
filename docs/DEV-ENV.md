# Developer environment overrides

XZram does not load a `.env` file. For tests and local experiments, set these
variables in the shell (or test harness) as needed.

| Variable | Purpose |
|----------|---------|
| `XZRAM_DATA_DIR` | Override `/var/lib/xzram` (pending config, snapshots). Used heavily in unit tests. |
| `XZRAM_ETC_ROOT` | Override filesystem root for `/etc` paths during snapshot/restore tests. |
| `XZRAM_DEV_HELPER` | Absolute path to a development `xzram-helper` binary. **Debug builds of the CLI only** (`cargo build` without `--release`), and only when `XZRAM_ALLOW_DEV_HELPER=1` is also set. Release builds always pkexec `/usr/libexec/xzram-helper`. |
| `XZRAM_ALLOW_DEV_HELPER` | Opt-in to allow `XZRAM_DEV_HELPER` for pkexec (debug builds only; ignored in release builds). |
| `XZRAM_CLI` | Absolute path to a development `xzram` CLI for the Qt GUI. Only compiled in when the GUI is configured with `-DXZRAM_DEV_OVERRIDES=ON`, and only used when `XZRAM_ALLOW_DEV_CLI=1` is also set. Default GUI builds always run the CLI from `XZRAM_CLI_PATH` (CMake cache, default `/usr/bin/xzram`) and never search `$PATH`. |
| `XZRAM_ALLOW_DEV_CLI` | Opt-in to allow `XZRAM_CLI` override in xzram-qt (`XZRAM_DEV_OVERRIDES=ON` builds only). |
| `RUST_LOG` | Tracing filter for `xzram`, `xzram-helper`, and `xzramd` (via `tracing-subscriber`). Default when unset: `warn`. |

Example (scratch dirs for a manual apply dry-run in tests):

```bash
export XZRAM_DATA_DIR=/tmp/xzram-data
export XZRAM_ETC_ROOT=/tmp/xzram-etc
mkdir -p "$XZRAM_DATA_DIR" "$XZRAM_ETC_ROOT"
```

Debug logging:

```bash
RUST_LOG=debug cargo run -p xzram-cli -- status
# or narrow: RUST_LOG=xzram_cli=debug,xzram=debug
```

When pointing the CLI at a locally built helper:

```bash
export XZRAM_ALLOW_DEV_HELPER=1
export XZRAM_DEV_HELPER=$PWD/target/debug/xzram-helper
cargo run -p xzram-cli -- status   # debug build: dev helper override honoured
```

Before any privileged action the CLI runs `<helper> --version` and refuses helpers older than 0.4.2 (they silently ignored payload fields they did not know). From 0.4.2 the helper and `xzramd` reject unknown fields themselves (`validation::from_json_strict`), so a CLI newer than the installed package fails loudly instead of half-applying. When developing against an older installed package, use the dev-helper override above or upgrade the package.

GUI against a local CLI (the override is compiled out unless requested):

```bash
cmake -S gui -B build-gui -DXZRAM_DEV_OVERRIDES=ON && cmake --build build-gui
XZRAM_ALLOW_DEV_CLI=1 XZRAM_CLI=$PWD/target/debug/xzram build-gui/xzram-qt/xzram-qt
```

Never ship a GUI built with `XZRAM_DEV_OVERRIDES=ON` or a debug CLI: both let an
environment variable choose which binary performs privileged operations.

Do not commit machine-specific paths. Prefer `DESTDIR` installs over root writes
into the source tree so `build-gui/` stays owned by your user.
