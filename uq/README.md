# uQ integration

This folder adds one page to [uQ](https://github.com/asafelobotomy/uq) Settings. It is optional:
nothing here is used unless uQ is installed, and the `xzram` package ships it only as a set of
files under `/usr/share/uq/settings/pages/xzram/`.

## Memory and swap (`pages/xzram/`)

Under **Apps** in uQ Settings, the page shows:

- memory and swap in use, and each compressed-memory (zram) device;
- the zram size (a quarter, half, three quarters of memory or as much as memory) and the
  compression algorithm;
- swappiness, 0 to 200, applied with **Apply**;
- the swap areas on the computer, read-only;
- **Refresh** and **More in XZram**, which opens the XZram app when `xzram-gui` is installed.

Not on the page, on purpose: creating, resizing or removing swap files. Those are disk
changes and stay in the XZram app and the `xzram` command.

### How it talks to XZram

The page reads and writes through XZram's own system service over D-Bus
(`io.github.XZram1`, `/io/github/XZram`), the same one the GUI and `xzram --dbus` use.

- Reads: `GetStatus`, `GetZramConfig`, `GetSysctl` and `ListSwaps`. These need no password.
- Writes: `ApplyNowZram` (size or compression) and `ApplyNowSysctl` (swappiness). The service
  asks for an administrator password through polkit, and uQ allows these two calls up to
  120 seconds for that. A change of zram size or compression moves swap in use first, so the
  page asks before it starts.
- The page sends the whole current zram configuration back with one field changed. The
  `xzram zram set` command is not used because it resets every flag it isn't given.

### Files

| File | What it is |
|------|------------|
| `pages/xzram/page.toml` | the manifest: search rows, `[requires]`, the `[dbus]` allowlist |
| `pages/xzram/Page.qml` | the page |
| `pages/xzram/StatusSection.qml`, `SwapListSection.qml`, `ConfirmDialog.qml` | its parts |
| `pages/xzram/logic.js` | parsing, value checks, payloads and wording (no Qt calls) |

### Testing

`tests/uq/` holds the uQ page test kit (stub components, a fake `ctx`) and this page's tests:

```bash
node --test 'tests/uq/*.test.mjs'                  # logic.js and the manifest
QT_QPA_PLATFORM=offscreen qmltestrunner -import tests/uq/stub -input tests/uq
uqctl integrations check --no-programs uq/        # the folder against uQ's rules
```

CI runs all three in the `uq` job. The kit was copied from uQ v0.3.0-rc.1; refresh it when
moving to a newer uQ.
