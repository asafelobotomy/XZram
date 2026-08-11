# Developer environment overrides

XZram does not load a `.env` file. For tests and local experiments, set these
variables in the shell (or test harness) as needed.

| Variable | Purpose |
|----------|---------|
| `XZRAM_DATA_DIR` | Override `/var/lib/xzram` (pending config, snapshots). Used heavily in unit tests. |
| `XZRAM_ETC_ROOT` | Override filesystem root for `/etc` paths during snapshot/restore tests. |
| `XZRAM_DEV_HELPER` | Absolute path to a development `xzram-helper` binary. Only used when `XZRAM_ALLOW_DEV_HELPER=1` is also set (pkexec must otherwise use `/usr/libexec/xzram-helper`). |
| `XZRAM_ALLOW_DEV_HELPER` | Opt-in to allow `XZRAM_DEV_HELPER` for pkexec (development only). |
| `XZRAM_CLI` | Absolute path to a development `xzram` CLI for the Qt GUI. Only used when `XZRAM_ALLOW_DEV_CLI=1` is also set (GUI otherwise prefers `/usr/bin/xzram`). |
| `XZRAM_ALLOW_DEV_CLI` | Opt-in to allow `XZRAM_CLI` override in xzram-qt (development only). |
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
export XZRAM_ALLOW_DEV_CLI=1
export XZRAM_CLI=$PWD/target/debug/xzram
cargo run -p xzram-cli -- status
# GUI: XZRAM_ALLOW_DEV_CLI=1 XZRAM_CLI=$PWD/target/debug/xzram build-gui/xzram-qt/xzram-qt
```

Do not commit machine-specific paths. Prefer `DESTDIR` installs over root writes
into the source tree so `build-gui/` stays owned by your user.
