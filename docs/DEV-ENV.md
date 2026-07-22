# Developer environment overrides

XZram does not load a `.env` file. For tests and local experiments, set these
variables in the shell (or test harness) as needed.

| Variable | Purpose |
|----------|---------|
| `XZRAM_DATA_DIR` | Override `/var/lib/xzram` (pending config, snapshots). Used heavily in unit tests. |
| `XZRAM_ETC_ROOT` | Override filesystem root for `/etc` paths during snapshot/restore tests. |
| `XZRAM_DEV_HELPER` | Absolute path to a development `xzram-helper` binary; bypasses system helper lookup. |

Example (scratch dirs for a manual apply dry-run in tests):

```bash
export XZRAM_DATA_DIR=/tmp/xzram-data
export XZRAM_ETC_ROOT=/tmp/xzram-etc
mkdir -p "$XZRAM_DATA_DIR" "$XZRAM_ETC_ROOT"
```

When pointing the CLI at a locally built helper:

```bash
export XZRAM_DEV_HELPER=$PWD/target/debug/xzram-helper
cargo run -p xzram-cli -- status
```

Do not commit machine-specific paths. Prefer `DESTDIR` installs over root writes
into the source tree so `build-gui/` stays owned by your user.
