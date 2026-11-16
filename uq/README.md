# uQ integration

This folder adds one page to [uQ](https://github.com/asafelobotomy/uq) Settings. It is optional:
nothing here is used unless uQ is installed, and the `xzram` package ships it only as a set of
files under `/usr/share/uq/settings/pages/xzram/`. It needs a uQ with `uq_api = 3` (on uQ's main
branch; an older uQ skips the page with "needs a newer uQ").

## Memory and swap (`pages/xzram/`)

Under **Apps** in uQ Settings, five tabs, laid out like Settings' own pages:

| Tab | What it does |
|-----|--------------|
| **Overview** | memory and swap in use (re-read every few seconds while the tab is open), memory pressure, each compressed-memory (zram) device with how well it squeezes, and the health check (`xzram doctor`) |
| **Compressed memory** | the zram size (a quarter, half, three quarters of memory or as much as memory), the compression algorithm, its priority, set-up when there is none, and turning it off |
| **Tuning** | swappiness, swap read-ahead, file cache pressure and the two watermark settings, each with its range, what it does and the usual choice |
| **Swap files** | the swap files in `/etc/fstab` (resize, remove), other swap areas (read-only) and **Add a swap file** |
| **Snapshots** | take a snapshot, restore or delete one, trim to the newest N, and **Undo the last change** (`xzram rollback`) |

Every change shows what it will do (uQ's confirmation sheet, filled from `--dry-run`), asks
first, and streams a progress bar while it runs. The page never forces anything: XZram's own
refusals are shown as they are. The one refusal with a way forward, not enough free memory to
move swap back, offers **Save for the next restart** (`--at-boot`), which writes the
configuration and leaves the running device alone.

### How it talks to XZram

Everything goes through the `xzram` command line; there is no D-Bus table. The commands are
in `page.toml`:

- Reads (no password): `status --json`, `config show --json` (followed while
  `/etc/systemd/zram-generator.conf` and `/etc/sysctl.d/99-xzram.conf` change),
  `swapfile list --json` (followed with `/etc/fstab`), `snapshot list --json`, `doctor --json`.
- Previews: `config set … --dry-run --json`, `zram disable … --dry-run --json`,
  `swapfile create|resize|remove … --dry-run --json`, `snapshot restore|delete|prune … --dry-run --json`
  and `rollback --dry-run --json`. Each prints one plan document
  (`{summary, changes[{group, label, detail, bytes, reversible}], would_refuse, reason}`).
  A plan that would be refused still exits 0.
- Writes: the same commands with `--dbus --ndjson` and the `--dry-run` taken off. Each prints
  `start`, `progress` and a last `done` or `error` event. While zram or a swap file is being
  switched off, `progress` carries the share of what was in swap that has moved back into
  memory (measured, not guessed). XZram asks for an administrator password through polkit; uQ
  allows these commands up to 10 minutes.
- Taking a snapshot only reads settings and stores a copy, so it neither asks nor streams.
- Zram changes go through `xzram config set`, which sends the whole current zram configuration
  back with one field changed. `xzram zram set` is not used because it resets every flag it
  isn't given.

Not on the page: staged changes (`xzram pending`, `xzram apply`), the recommended defaults
(`xzram defaults …`) and creating swap on Btrfs with a prepared folder (`--prepare`). Those
stay in the XZram app and the `xzram` command.

### Files

| File | What it is |
|------|------------|
| `pages/xzram/page.toml` | the manifest: search rows, `[requires]`, every `[[command]]` with its argument patterns, confirmation text and preview |
| `pages/xzram/Page.qml` | the page: five tabs, and the one place that previews, streams and words every change |
| `pages/xzram/OverviewSection.qml`, `ZramDevices.qml`, `HealthSection.qml` | the Overview cards |
| `pages/xzram/ZramSection.qml`, `TuningSection.qml`, `SwapFilesSection.qml`, `SnapshotsSection.qml` | the other tabs |
| `pages/xzram/Outcome.qml`, `NumberBox.qml` | the progress bar and answer under a row; a number box that knows what is typed |
| `pages/xzram/logic.js` | parsing, value checks, argument patterns and wording (no Qt calls) |

### Testing

`tests/uq/` holds the uQ page test kit (stub components, a fake `ctx`) and this page's tests:

```bash
node --test 'tests/uq/*.test.mjs'                  # logic.js and the manifest
QT_QPA_PLATFORM=offscreen qmltestrunner -import tests/uq/stub -input tests/uq
uqctl integrations check --no-programs uq/        # the folder against uQ's rules
cargo test -p xzram-cli                           # the plans, the settings and the progress events
```

CI runs all four. The kit follows uQ's `templates/settings-page-tests` at the commit CI
pins (`UQ_REV` in `.github/workflows/ci.yml`); refresh it when that moves.
