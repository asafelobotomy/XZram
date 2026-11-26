// Tests for uq/pages/xzram/logic.js, plans.js and page.toml. Run: node --test 'tests/uq/*.test.mjs'
import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import { test } from "node:test";
import { loadLogic } from "./load-logic.mjs";
import { parseToml } from "./toml-lite.mjs";

const pageDir = new URL("../../uq/pages/xzram/", import.meta.url);
// logic.js reads and checks; plans.js words plans and progress. One API for the tests.
const L = { ...loadLogic(new URL("logic.js", pageDir)), ...loadLogic(new URL("plans.js", pageDir)) };
const manifest = parseToml(readFileSync(new URL("page.toml", pageDir), "utf8"));
// Values made inside the vm context have another prototype; compare plain copies.
const plain = (v) => JSON.parse(JSON.stringify(v));
const fx = (name) => JSON.parse(readFileSync(new URL(`fixtures/${name}.json`, import.meta.url), "utf8"));
const ok = (name) => ({ code: 0, output: fx(name) });
const command = (id) => manifest.command.find((c) => c.id === id);

// ---- page.toml and logic.js agree ---------------------------------------------------------------

test("page is uq_api 3 and needs the xzram program", () => {
  assert.equal(manifest.uq_api, 3);
  assert.deepEqual(manifest.requires.bins, ["xzram"]);
});

test("every argument pattern in logic.js is the one in page.toml", () => {
  for (const [id, table] of Object.entries(plain(L.PATTERNS))) {
    const spec = command(id);
    assert.ok(spec, `page.toml has no command ${id}`);
    assert.deepEqual(
      (spec.args || []).map((a) => [a.name, a.pattern]),
      Object.entries(table),
      `patterns of ${id} differ`,
    );
  }
});

test("every command the page runs with a value has its patterns in logic.js", () => {
  for (const c of manifest.command) {
    if (c.args || c.confirm || c.id.startsWith("plan-") || c.id === "snapshot-create") {
      assert.ok(L.PATTERNS[c.id], `logic.js does not check ${c.id}`);
    }
  }
});

test("every write previews itself and streams; every preview is a read", () => {
  const writes = manifest.command.filter((c) => c.confirm);
  assert.ok(writes.length >= 12);
  for (const w of writes) {
    assert.equal(w.output, "ndjson", `${w.id} should stream`);
    const plan = command(w.preview);
    assert.ok(plan, `${w.id} has no preview`);
    assert.equal(plan.output, "json");
    assert.ok(!plan.confirm && !plan.preview, `${plan.id} must be a plain read`);
    assert.ok(plan.argv.includes("--dry-run"), `${plan.id} is not a dry run`);
    assert.deepEqual((plan.args || []).map((a) => a.name), (w.args || []).map((a) => a.name));
    assert.ok(!w.argv.includes("--force"), `${w.id} must never force`);
    assert.ok(w.argv.includes("--ndjson"));
  }
});

test("every page setting is a row the page has", () => {
  const rows = readdirSync(pageDir)
    .filter((f) => f.endsWith(".qml"))
    .flatMap((f) => [...readFileSync(new URL(f, pageDir), "utf8").matchAll(/settingId:\s*([^\n]+)/g)].map((m) => m[1]))
    .join("\n");
  for (const s of manifest.setting) {
    assert.ok(rows.includes(`"${s.id}"`) || (s.id === "tuning" && rows.includes('"tuning"')), `no row for ${s.id}`);
  }
});

test("tst_page.qml lists the commands that confirm", () => {
  const plan = manifest.command.filter((c) => c.confirm).map((c) => c.id).sort();
  const test = readFileSync(new URL("tst_page.qml", import.meta.url), "utf8");
  const listed = /confirmed:\s*\[([^\]]*)\]/.exec(test);
  assert.ok(listed, "tst_page.qml does not list the commands that confirm");
  assert.deepEqual([...listed[1].matchAll(/"([a-z-]+)"/g)].map((m) => m[1]).sort(), plan);
});

test("the page's own text has no jargon", () => {
  const BANNED = /D-?Bus|polkit|JSON|systemctl|xzramd|sysctl|zram-generator|pkexec|\bndjson\b/i;
  const found = [];
  for (const f of readdirSync(pageDir).filter((f) => f.endsWith(".qml"))) {
    for (const line of readFileSync(new URL(f, pageDir), "utf8").split("\n")) {
      if (/\btext:|placeholderText:|title:|help:|detail:|label:/.test(line)) {
        found.push(...[...line.matchAll(/"((?:[^"\\]|\\.)*)"/g)].map((m) => m[1]).filter((s) => / /.test(s)));
      }
    }
  }
  for (const c of manifest.command) if (c.confirm) found.push(c.confirm);
  for (const s of manifest.setting) found.push(s.label);
  for (const t of L.TUNING) found.push(t.label, t.help);
  assert.ok(found.length > 40, `only ${found.length} strings found`);
  for (const s of found) assert.ok(!BANNED.test(s), `jargon in: ${s}`);
});

// ---- reading what xzram prints -------------------------------------------------------------------

test("status: memory, swap, pressure and zram devices", () => {
  const s = plain(L.parseStatus(ok("status-active")));
  assert.equal(s.ok, true);
  assert.equal(s.memory.totalBytes, 16473776 * 1024);
  assert.equal(s.memory.usedBytes, (16473776 - 8236888) * 1024);
  assert.equal(s.memory.swapUsedBytes, 1048576 * 1024);
  assert.equal(s.zram[0].ratio, 3);
  assert.equal(s.zram[0].active, true);
  assert.equal(s.swaps.length, 2);
  assert.equal(s.pressure, 12.5);
});

test("status on a computer with no swap", () => {
  const s = plain(L.parseStatus(ok("status")));
  assert.equal(s.ok, true);
  assert.equal(s.memory.swapTotalBytes, 0);
  assert.equal(s.zram[0].active, false);
});

test("wrong shapes become a failed read, never an exception", () => {
  for (const bad of [undefined, null, {}, { code: 0 }, { code: 0, output: 5 }, { error: "boom" }, { code: 1, output: { ok: false, error: "no" } }]) {
    for (const parse of [L.parseStatus, L.parseConfig, L.parseDoctor, L.parseSwapfiles, L.parseSnapshots]) {
      const r = plain(parse(bad));
      assert.equal(r.ok, false);
      assert.equal(typeof r.error, "string");
    }
  }
  assert.equal(plain(L.parseStatus({ error: "xzram: gone" })).error, "xzram: gone");
});

test("config: zram settings and tuning values", () => {
  const c = plain(L.parseConfig(ok("config-active")));
  assert.deepEqual(c.zram, { configured: true, size: "ram / 2", algorithm: "zstd", priority: 100 });
  assert.equal(c.tuning["swappiness"], 60);
  assert.equal(c.tuning["page-cluster"], 3);
  assert.equal(c.pending, false);
  const none = plain(L.parseConfig(ok("config")));
  assert.equal(none.zram.configured, false);
  assert.equal(none.zram.size, null);
});

test("doctor is read even when it exits 3, problems first", () => {
  const d = plain(L.parseDoctor({ code: 3, error: "exit 3", output: { healthy: false, summary: "2 problems", issues: [
    { severity: "warning", message: "w", suggestion: "do w" },
    { severity: "error", message: "e" },
  ] } }));
  assert.equal(d.ok, true);
  assert.equal(d.healthy, false);
  assert.deepEqual(d.issues.map((i) => i.severity), ["error", "warning"]);
  assert.equal(plain(L.parseDoctor(ok("doctor"))).issues.length, 2);
});

test("swap files and snapshots", () => {
  assert.deepEqual(plain(L.parseSwapfiles(ok("swapfiles")).files), [{ path: "/swap/swapfile", sizeMb: 4096, priority: 10 }]);
  const s = plain(L.parseSnapshots(ok("snapshots")).snapshots);
  assert.equal(s.length, 2);
  assert.equal(s[0].before, true);
  assert.equal(s[0].when, "2026-11-16 10:15");
  assert.equal(s[1].trigger, "Taken by hand");
  // an id that isn't one this page could pass back is dropped
  assert.equal(plain(L.parseSnapshots({ code: 0, output: [{ id: "../x", label: "x" }] }).snapshots).length, 0);
  assert.equal(plain(L.parseSwapfiles({ code: 0, output: [{ path: "relative", size_mb: 1 }] }).files).length, 0);
});

test("text from xzram is cut and stripped of control characters", () => {
  const long = "x".repeat(1000);
  const d = plain(L.parseDoctor({ code: 0, output: { issues: [{ severity: "warning", message: "a\u0000b\n" + long }] } }));
  assert.ok(d.issues[0].message.length <= 300);
  assert.ok(!/[\u0000-\u001f]/.test(d.issues[0].message));
});

// ---- words and numbers -----------------------------------------------------------------------------

test("sizes use binary units like the xzram command", () => {
  assert.equal(L.size(0), "0 bytes");
  assert.equal(L.size(1536), "1.5 KiB");
  assert.equal(L.size(3 * 1024 ** 3), "3.0 GiB");
  assert.equal(L.size(-5), "0 bytes");
  assert.equal(L.size("x"), "0 bytes");
});

test("bars are clamped", () => {
  assert.equal(L.fraction(5, 10), 0.5);
  assert.equal(L.fraction(20, 10), 1);
  assert.equal(L.fraction(1, 0), 0);
});

test("pressure is worded in three steps", () => {
  assert.match(L.pressureText(0), /^None/);
  assert.match(L.pressureText(3), /^Some/);
  assert.match(L.pressureText(40), /^High/);
});

test("size and algorithm menus always show what is in force", () => {
  assert.equal(plain(L.sizeChoices("ram / 2")).length, 4);
  assert.equal(L.sizeIndex("ram/2"), 1, "spacing doesn't matter");
  const custom = plain(L.sizeChoices("min(ram, 4096)"));
  assert.equal(custom[0].label, "Custom: min(ram, 4096)");
  assert.equal(L.sizeIndex("min(ram, 4096)"), 0);
  assert.equal(plain(L.sizeChoices(null))[0].label, "Default");
  assert.equal(L.algorithmChoices("zstd").length, 5);
  assert.equal(plain(L.algorithmChoices("brotli"))[0], "brotli");
  assert.equal(plain(L.algorithmChoices("-evil")).includes("-evil"), false);
  assert.equal(L.algorithmIndex("lz4"), 1);
  assert.equal(L.algorithmIndex(null), 0);
});

test("tuning values are checked against each key's range", () => {
  assert.equal(L.checkTuning("swappiness", "200"), "");
  assert.match(L.checkTuning("swappiness", "201"), /0 to 200/);
  assert.match(L.checkTuning("swappiness", "-1"), /whole number/);
  assert.match(L.checkTuning("swappiness", "6x"), /whole number/);
  assert.match(L.checkTuning("vfs-cache-pressure", "0"), /1 to 1000/);
  assert.equal(L.checkTuning("page-cluster", " 8 "), "");
  assert.match(L.checkTuning("nope", "1"), /isn't known/);
  assert.equal(L.checkPriority("32767"), "");
  assert.match(L.checkPriority("32768"), /0 to 32767/);
});

test("a new swap file needs an allowed path and a sane size", () => {
  assert.equal(L.checkSwapfile("/swap/swapfile", "4"), "");
  assert.equal(L.checkSwapfile("/swapfile", "0.5"), "");
  assert.match(L.checkSwapfile("/etc/swapfile", "4"), /goes in/);
  assert.match(L.checkSwapfile("/swap/x y", "4"), /goes in/);
  assert.match(L.checkSwapfile("/swap/swapfile", "0"), /size from/);
  assert.match(L.checkSwapfile("/swap/swapfile", "abc"), /size from/);
  assert.equal(L.mibOf("4"), "4096");
  assert.equal(L.mibOf("0.5"), "512");
});

test("arguments are checked as uQ will", () => {
  assert.equal(L.argsProblem("set-zram", ["zram-size", "ram / 2"]), "");
  assert.equal(L.argsProblem("set-zram", ["zram-size", "-ram"]) !== "", true);
  assert.equal(L.argsProblem("set-zram", ["swappiness", "10"]) !== "", true);
  assert.equal(L.argsProblem("set-zram", ["zram-size"]) !== "", true);
  assert.equal(L.argsProblem("set-zram", ["zram-size", "ram]\nx"]) !== "", true);
  assert.equal(L.argsProblem("set-tuning", ["swappiness", "100"]), "");
  assert.equal(L.argsProblem("snapshot-restore", ["20260116T101500-pre_apply"]), "");
  assert.equal(L.argsProblem("snapshot-restore", ["../etc"]) !== "", true);
  assert.equal(L.argsProblem("swapfile-create", ["4096", "/swap/swapfile"]), "");
  assert.equal(L.argsProblem("swapfile-create", ["/swap/swapfile", "4096"]) !== "", true);
  assert.equal(L.argsProblem("rm-rf", []) !== "", true);
});

// ---- plans, progress and outcomes -----------------------------------------------------------------------

test("a refused-for-memory plan is the one that offers the next restart", () => {
  const ok1 = plain(L.parsePlan(ok("plan-ok")));
  assert.deepEqual([ok1.ok, ok1.refuse, ok1.memory], [true, false, false]);
  const mem = plain(L.parsePlan(ok("plan-memory")));
  assert.deepEqual([mem.ok, mem.refuse, mem.memory], [true, true, true]);
  assert.equal(L.refusalText(mem), L.LOW_MEMORY);
  const other = plain(L.parsePlan({ code: 0, output: { would_refuse: true, reason: "There is no snapshot x." } }));
  assert.equal(other.memory, false);
  assert.equal(L.refusalText(other), "Not changed. There is no snapshot x.");
  assert.equal(plain(L.parsePlan({ code: 1, error: "exit 1" })).ok, false);
});

test("progress events become a bar", () => {
  assert.deepEqual(plain(L.progressOf({ event: "start", label: "Changing compressed memory" })), { label: "Changing compressed memory", percent: -1, detail: "" });
  assert.deepEqual(plain(L.progressOf({ event: "progress", label: "Moving", percent: 40, detail: "1.2 GiB left to move" })), { label: "Moving", percent: 40, detail: "1.2 GiB left to move" });
  assert.equal(L.progressOf({ event: "progress", percent: 400 }).percent, 99, "never 100 before the end");
  assert.equal(L.progressOf({ event: "progress", percent: null }).percent, -1);
  assert.equal(L.progressOf({ event: "done" }), null);
  assert.equal(L.progressOf("x"), null);
});

test("outcomes: done, refused for memory, denied, timed out", () => {
  const done = plain(L.outcome({ event: "done", ok: true }, { code: 0 }, "Done."));
  assert.deepEqual([done.message, done.failed], ["Done.", false]);
  const mem = plain(L.outcome({ event: "error", kind: "memory", error: "not enough free memory: …" }, { code: 1 }, "Done."));
  assert.deepEqual([mem.message, mem.failed, mem.kind], [L.LOW_MEMORY, true, "memory"]);
  assert.match(mem.detail, /not enough free memory/);
  const denied = plain(L.outcome({ event: "error", kind: "denied", error: "Authorization denied" }, { code: 1 }, "Done."));
  assert.match(denied.message, /permission wasn't given/);
  const other = plain(L.outcome({ event: "error", kind: "other", error: "disk full" }, { code: 1 }, "Done."));
  assert.deepEqual([other.message, other.failed, other.detail], ["Couldn't make that change.", true, "disk full"]);
  const timeout = plain(L.outcome(null, { error: "timed out after 600s" }, "Done."));
  assert.equal(timeout.failed, false);
  assert.match(timeout.message, /Still working/);
  assert.equal(plain(L.outcome(null, { error: "killed" }, "Done.")).failed, true);
  assert.equal(plain(L.outcome(null, { code: 0 }, "Done.")).failed, false);
});

test("an unknown error kind is treated as any other failure", () => {
  assert.equal(plain(L.outcome({ event: "error", kind: "<b>x</b>", error: "y" }, {}, "Done.")).kind, "other");
});

test("notes are copied, not changed", () => {
  const a = { x: 1 };
  const b = L.withNote(a, "y", { m: 1 });
  assert.deepEqual(plain(a), { x: 1 });
  assert.deepEqual(plain(b), { x: 1, y: { m: 1 } });
  assert.deepEqual(plain(L.withNote(b, "y", null)), { x: 1 });
});

test("what a finished change says", () => {
  assert.equal(L.doneText("zram-size", "ram"), "Compressed memory size changed.");
  assert.equal(L.doneText("swappiness", "100"), "Swappiness set to 100.");
});
