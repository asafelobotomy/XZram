// Tests for uq/pages/xzram/logic.js and page.toml. Run: node --test 'tests/uq/*.test.mjs'
import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import { test } from "node:test";
import { loadLogic } from "./load-logic.mjs";

const pageDir = new URL("../../uq/pages/xzram/", import.meta.url);
const L = loadLogic(new URL("logic.js", pageDir));
// Values made inside the vm context have another prototype; compare plain copies.
const plain = (v) => JSON.parse(JSON.stringify(v));

/** A reply as ctx.dbus.call gives it: a{sv} holding one `json` string. */
const reply = (doc) => ({ value: { json: JSON.stringify(doc) } });

/** A tiny reader for the subset of TOML page.toml uses. */
function parseToml(text) {
  const root = {};
  let table = root;
  for (const raw of text.split("\n")) {
    const line = raw.trim();
    if (line === "" || line.startsWith("#")) continue;
    let m = line.match(/^\[\[([A-Za-z_.-]+)\]\]$/);
    if (m) {
      table = {};
      (root[m[1]] ??= []).push(table);
      continue;
    }
    m = line.match(/^\[([A-Za-z_.-]+)\]$/);
    if (m) {
      table = root[m[1]] = {};
      continue;
    }
    m = line.match(/^([A-Za-z_][A-Za-z0-9_-]*)\s*=\s*(.*?)\s*(#.*)?$/);
    assert.ok(m, `unreadable line: ${line}`);
    let value = m[2];
    // Strip a trailing comment that follows a closing quote, bracket or brace.
    value = value.replace(/^(.*[\]}"0-9])\s+#.*$/, "$1");
    if (value.startsWith("{")) value = value.replace(/([A-Za-z_]+)\s*=/g, '"$1":');
    table[m[1]] = JSON.parse(value);
  }
  return root;
}

const manifest = parseToml(readFileSync(new URL("page.toml", pageDir), "utf8"));

const STATUS = {
  swaps: [{ name: "/dev/zram0", swap_type: "partition", size_bytes: 8589934592, used_bytes: 1073741824, priority: 100 }],
  zram_devices: [
    {
      name: "zram0",
      algorithm: "zstd",
      disk_size_bytes: 8589934592,
      data_bytes: 3221225472,
      compressed_bytes: 1073741824,
      total_bytes: 1181116006,
      mem_used_max_bytes: 1181116006,
      mount_point: "[SWAP]",
    },
  ],
  memory: { mem_total_kb: 16777216, mem_available_kb: 8388608, swap_total_kb: 8388608, swap_free_kb: 7340032 },
};
const ZRAM = {
  device: "zram0",
  zram_size: "ram / 2",
  zram_resident_limit: null,
  compression_algorithm: "zstd",
  swap_priority: 100,
  fs_type: null,
  mount_point: null,
};

test("parses_status", () => {
  const s = L.parseStatus(reply(STATUS));
  assert.equal(s.ok, true);
  assert.equal(s.memory.totalBytes, 16 * 1024 ** 3);
  assert.equal(s.zram[0].algorithm, "zstd");
  assert.deepEqual(plain(L.statusLines(s)), [
    "Memory: 8 GB in use of 16 GB",
    "Swap: 1 GB in use of 8 GB",
    "zram0: zstd, 8 GB, 3 GB stored in 1 GB (3 to 1)",
  ]);
});

test("parses_status_defensively", () => {
  for (const bad of [
    undefined,
    null,
    {},
    { error: "boom" },
    { value: {} },
    { value: { json: 7 } },
    { value: { json: "{not json" } },
    reply(null),
    reply([]),
    reply({ memory: 3 }),
  ]) {
    const r = L.parseStatus(bad);
    assert.equal(r.ok, false, JSON.stringify(bad));
    assert.equal(typeof r.error, "string");
  }
  // Wrong types inside become zero or are skipped, never an exception.
  const odd = L.parseStatus(reply({ memory: { mem_total_kb: "x" }, zram_devices: [1, null, { name: 5 }] }));
  assert.equal(odd.ok, true);
  assert.equal(odd.memory.totalBytes, 0);
  assert.equal(odd.zram.length, 1);
});

test("caps_rows_and_text", () => {
  const many = Array.from({ length: 900 }, (_, i) => ({ name: "n".repeat(500) + i, active: true }));
  const r = L.parseSwaps(reply(many));
  assert.equal(r.swaps.length, 500);
  assert.ok(r.swaps[0].name.length <= 201);
});

test("parses_zram_config_sysctl_and_swaps", () => {
  assert.deepEqual(plain(L.parseZramConfig(reply(ZRAM))), { ok: true, config: ZRAM });
  assert.deepEqual(plain(L.parseZramConfig(reply(null))), { ok: true, config: null });
  assert.equal(L.parseZramConfig(reply({ device: "../x" })).ok, false);
  assert.equal(L.parseSysctl(reply({ swappiness: 60, page_cluster: 0 })).swappiness, 60);
  assert.equal(L.parseSysctl(reply({})).swappiness, null);
  assert.equal(L.parseSysctl(reply({ swappiness: 9999 })).swappiness, null);
  assert.equal(L.parseSysctl(reply([])).ok, false);
  const swaps = L.parseSwaps(
    reply([
      { name: "/dev/zram0", swap_type: "partition", size_bytes: 2048, used_bytes: 1024, priority: 100, source: "active", active: true },
      { name: "/swapfile", swap_type: "file", size_bytes: 0, used_bytes: 0, priority: -2, source: "fstab", active: false },
    ]),
  );
  assert.deepEqual(plain(swaps.swaps.map(L.swapLine)), [
    "/dev/zram0: partition, 1 KB used of 2 KB",
    "/swapfile: file, not in use right now",
  ]);
  assert.equal(L.parseSwaps(reply({})).ok, false);
});

test("size_and_swappiness_validation", () => {
  for (const ok of ["ram / 2", "min(ram / 2, 4096)", "ram * 3 / 4", "ram", "1024", "max(ram/4,512)"])
    assert.equal(L.checkSize(ok), "", ok);
  for (const bad of ["", "   ", "ram]\n[x", "a\nb", "$(id)", "ram;rm", "x".repeat(201), 5, null])
    assert.notEqual(L.checkSize(bad), "", String(bad));
  for (const ok of [0, 1, 60, 200, "60", "0", "200"]) assert.equal(L.checkSwappiness(ok), "", String(ok));
  for (const bad of [-1, 201, 1.5, "abc", "", "-1", "1e2", NaN, Infinity, null, undefined, "2000"])
    assert.notEqual(L.checkSwappiness(bad), "", String(bad));
  for (const ok of ["zstd", "lz4", "lzo-rle", "lzo", "842"]) assert.equal(L.checkAlgorithm(ok), "", ok);
  for (const bad of ["", "-rf", "a b", "a]", "a\nb", "../x", "x".repeat(65), "zstd\n", null])
    assert.notEqual(L.checkAlgorithm(bad), "", String(bad));
});

test("rejects_hostile_values", () => {
  for (const size of ["ram]\n[Service]", "ram\nzram-size = 1", "$(id)", "`id`", "x".repeat(5000), "ram | id"]) {
    const r = L.buildZramConfig(ZRAM, { size });
    assert.equal(r.ok, false, size);
    assert.equal(r.json, undefined);
  }
  for (const algorithm of ["zstd]\n", "-rf", "--force", "a\nb", "../x", "x".repeat(5000)]) {
    assert.equal(L.buildZramConfig(ZRAM, { algorithm }).ok, false, algorithm);
  }
  for (const v of ["-1", "60; reboot", 1e9]) assert.equal(L.buildSysctl(v).ok, false, String(v));
  assert.equal(L.buildZramConfig({ device: "../zram0" }, { size: "ram" }).ok, false);
  assert.equal(L.buildZramConfig(null, { size: "ram" }).ok, false);
  assert.equal(L.buildZramConfig(ZRAM, {}).ok, false);
});

test("builds_whole_zram_config_with_one_change", () => {
  const size = L.buildZramConfig(ZRAM, { size: "ram * 3 / 4" });
  assert.equal(size.ok, true);
  assert.equal(size.kind, "zram-size");
  // Every other field is kept, so XZram doesn't reset the algorithm or priority.
  assert.deepEqual(JSON.parse(size.json), { ...ZRAM, zram_size: "ram * 3 / 4" });
  const algo = L.buildZramConfig(ZRAM, { algorithm: "lz4" });
  assert.equal(algo.kind, "zram-algorithm");
  assert.deepEqual(JSON.parse(algo.json), { ...ZRAM, compression_algorithm: "lz4" });
  assert.deepEqual(JSON.parse(L.buildSysctl(80).json), { swappiness: 80 });
  assert.deepEqual(JSON.parse(L.buildSysctl("80").json), { swappiness: 80 });
});

test("size_and_algorithm_choices", () => {
  const rows = plain(L.sizeChoices("ram / 2"));
  assert.deepEqual(rows.map((r) => r.value), ["ram / 4", "ram / 2", "ram * 3 / 4", "ram"]);
  assert.equal(L.sizeIndex("ram/2"), 1);
  const custom = plain(L.sizeChoices("min(ram / 2, 4096)"));
  assert.equal(custom[0].label, "Custom: min(ram / 2, 4096)");
  assert.equal(L.sizeIndex("min(ram / 2, 4096)"), 0);
  assert.equal(plain(L.sizeChoices(null))[0].label, "Default");
  assert.equal(L.sizeIndex(null), 0);
  assert.deepEqual(plain(L.algorithmChoices("zstd")), ["zstd", "lz4", "lzo-rle", "lzo", "842"]);
  assert.equal(L.algorithmChoices("lzo-rle")[L.algorithmIndex("lzo-rle")], "lzo-rle");
  assert.equal(L.algorithmChoices("deflate")[0], "deflate");
  assert.equal(L.algorithmIndex("deflate"), 0);
  assert.equal(L.algorithmChoices("bad name!")[0], "zstd");
});

test("describes_errors_in_plain_words", () => {
  const kind = (e) => L.describeError(e, "change swappiness").kind;
  assert.equal(kind("no answer within 2 s"), "pending");
  assert.equal(kind("org.freedesktop.DBus.Error.ServiceUnknown: The name is not activatable"), "unavailable");
  assert.equal(kind("io.github.XZram1 is not running"), "unavailable");
  assert.equal(kind("org.freedesktop.DBus.Error.AccessDenied: Not authorized"), "denied");
  assert.equal(kind("org.freedesktop.DBus.Error.InvalidArgs: swappiness must be 0-200"), "refused");
  assert.equal(L.describeError("weird", "change swappiness").message, "Couldn't change swappiness.");
  assert.equal(L.describeError("x".repeat(900), "read").raw.length, 201);
  assert.equal(L.isRetryable("no answer within 2 s"), true);
  assert.equal(L.isRetryable("InvalidArgs"), false);
});

test("manifest_has_no_commands", () => {
  // Every write is a D-Bus call (nothing for argument patterns to guard), so there are no
  // [[command]] tables and nothing to confirm in the manifest.
  assert.equal(manifest.command, undefined);
  assert.deepEqual(manifest.requires, { bins: ["xzram"] });
  assert.equal(manifest.requires.bus_name, undefined);
});

test("confirms_match_plan", () => {
  // uQ's own confirm exists only for [[command]]; the page asks for zram changes itself.
  assert.equal((manifest.command || []).filter((c) => c.confirm).length, 0);
  assert.deepEqual(plain(L.CONFIRMS), {
    "zram-size": "Change the zram size? Swap in use is moved first; this can take a while.",
    "zram-algorithm": "Change the zram compression? Swap in use is moved first; this can take a while.",
  });
  // Only the two zram changes are confirmed, and each is asked before the write: swappiness
  // changes nothing in use and is undone with the same control.
  const qml = readFileSync(new URL("Page.qml", pageDir), "utf8");
  assert.match(qml, /confirm\.open\(\)/);
  const runZram = qml.match(/function runZram\(\) \{[\s\S]*?\n    \}/)[0];
  assert.match(runZram, /ApplyNowZram/);
  const applyZramCalls = qml.match(/"ApplyNowZram"/g) || [];
  assert.equal(applyZramCalls.length, 1, "ApplyNowZram is only called from runZram, after Confirm");
  assert.match(qml, /onConfirmed: page\.runZram\(\)/);
});

test("dbus_writes_allowlisted", () => {
  const d = manifest.dbus;
  assert.equal(d.name, "io.github.XZram1");
  assert.equal(d.path, "/io/github/XZram");
  assert.equal(d.interface, "io.github.XZram.Manager");
  assert.equal(d.bus, "system");
  const reads = ["GetStatus", "GetZramConfig", "GetSysctl", "ListSwaps"];
  const writes = ["ApplyNowZram", "ApplyNowSysctl"];
  assert.deepEqual([...d.methods].sort(), [...reads, ...writes].sort());
  assert.deepEqual(d.timeout_ms, { ApplyNowZram: 120000, ApplyNowSysctl: 120000 });
  assert.equal(d.properties, undefined);
  // The staging and swap-file methods are never reachable from the page.
  for (const m of d.methods) assert.doesNotMatch(m, /Swapfile|Configure|Disable|^Apply$|Rollback|Snapshot/);
  // Every method the page code calls is listed.
  const code = ["Page.qml", "StatusSection.qml", "SwapListSection.qml", "ConfirmDialog.qml"]
    .map((f) => readFileSync(new URL(f, pageDir), "utf8"))
    .join("\n");
  const called = new Set([...code.matchAll(/(?:call\(|read\()\s*"([A-Za-z]+)"/g)].map((m) => m[1]));
  assert.deepEqual([...called].sort(), [...reads, ...writes].sort());
});

const JARGON = /D-?Bus|polkit|JSON|systemctl|xzramd|zram-generator|sysctl|sched_ext|EEVDF|\bbus\b/i;

/** Every double-quoted string literal in `line`. */
const literals = (line) => [...line.matchAll(/"((?:[^"\\]|\\.)*)"/g)].map((m) => m[1]);

test("page_strings_are_plain", () => {
  const files = readdirSync(pageDir).filter((f) => f.endsWith(".qml"));
  assert.ok(files.includes("Page.qml"));
  let checked = 0;
  for (const f of files) {
    for (const line of readFileSync(new URL(f, pageDir), "utf8").split("\n")) {
      if (!/\btext:|\btitle:|\bconfirm|\blabel:|\bnotice\s*=|\bproblem\s*=|"[A-Z][^"]* (?:the|a|an) [^"]*"/.test(line)) continue;
      if (/\bobjectName:|\bsettingId:|ctx\.|\.call\(|Logic\./.test(line) && !/\btext:|\blabel:|\btitle:/.test(line)) continue;
      for (const s of literals(line)) {
        assert.doesNotMatch(s, JARGON, `${f}: ${s}`);
        checked++;
      }
    }
  }
  assert.ok(checked > 10);
  // Texts built in logic.js.
  for (const s of Object.values(plain(L.CONFIRMS))) assert.doesNotMatch(s, JARGON);
  assert.doesNotMatch(L.STILL_WORKING, JARGON);
  for (const e of [
    "no answer within 2 s",
    "ServiceUnknown",
    "AccessDenied",
    "InvalidArgs",
    "other",
  ])
    assert.doesNotMatch(L.describeError(e, "change swappiness").message, JARGON);
  // The manifest's name, labels and confirms.
  const shown = [manifest.name, ...manifest.setting.map((s) => s.label), ...(manifest.command || []).map((c) => c.confirm || "")];
  for (const s of shown) assert.doesNotMatch(s, JARGON, s);
  assert.equal(manifest.name, "Memory and swap");
});

test("manifest_rows_have_keywords_and_unique_ids", () => {
  assert.deepEqual(manifest.setting.map((s) => s.id), ["zram-size", "zram-algorithm", "swappiness", "swaps"]);
  for (const s of manifest.setting) assert.ok(s.keywords.length > 0, s.id);
  assert.equal(manifest.id, "xzram");
  assert.equal(manifest.icon, "media-flash");
  assert.deepEqual(manifest.keywords, ["swap", "zram", "memory", "ram", "swappiness", "compression"]);
  // Every [[setting]] id is a row the page can scroll to.
  const qml = readFileSync(new URL("Page.qml", pageDir), "utf8") + readFileSync(new URL("SwapListSection.qml", pageDir), "utf8");
  for (const s of manifest.setting) assert.match(qml, new RegExp(`"${s.id}"`), s.id);
});

test("logic_has_no_qt_and_no_text_types", () => {
  const src = readFileSync(new URL("logic.js", pageDir), "utf8");
  assert.match(src, /^\.pragma library/);
  assert.doesNotMatch(src, /\bQt\./);
  assert.doesNotMatch(src, /createQmlObject/);
});
