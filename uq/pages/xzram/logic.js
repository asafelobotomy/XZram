.pragma library
// The Memory and swap page's pure logic: no Qt calls, so Node can test it
// (tests/uq/logic.test.mjs). Everything `xzram` prints is untrusted: it is read defensively, a
// wrong type becomes an empty or "couldn't read" result, lists are capped and strings are cut.

var ROW_LIMIT = 200;
var TEXT_LIMIT = 300;

/// Shown beside a row while its command runs.
var WORKING = "Working…";

/// XZram refuses to restart compressed memory when memory can't take back what is stored in it;
/// every such refusal says this (xzram::apply::MEMORY_REFUSAL).
var MEMORY_REFUSAL = "not enough free memory";

// ---------------------------------------------------------------------------------------------
// What can be chosen

/// Sizes of compressed memory the page offers: zram-generator expressions in terms of memory.
var SIZE_PRESETS = [
    { value: "ram / 4", label: "A quarter of memory" },
    { value: "ram / 2", label: "Half of memory" },
    { value: "ram * 3 / 4", label: "Three quarters of memory" },
    { value: "ram", label: "As much as memory" }
];

/// Compression algorithms the page offers (the kernel's usual names); the one in force is
/// added when it is another.
var ALGORITHMS = ["zstd", "lz4", "lzo-rle", "lzo", "842"];

var ALGORITHM_HELP = {
    "zstd": "Squeezes the most. A good choice for most computers.",
    "lz4": "Fastest, but squeezes less.",
    "lzo-rle": "A fast, light choice.",
    "lzo": "An older, light choice.",
    "842": "Made for IBM hardware."
};

/// The kernel settings on the Tuning tab: key, wording, range and the usual choice.
var TUNING = [
    { key: "swappiness", label: "Swappiness", min: 0, max: 200, usual: 60,
      help: "Higher values move memory to swap sooner. 60 is the usual choice; with compressed memory, higher works well." },
    { key: "page-cluster", label: "Swap read-ahead", min: 0, max: 8, usual: 0,
      help: "How many pages are read back from swap at once, as a power of two. 0 suits compressed memory." },
    { key: "vfs-cache-pressure", label: "File cache pressure", min: 1, max: 1000, usual: 100,
      help: "How eagerly the file-name caches are given back. 100 is the kernel's default." },
    { key: "watermark-scale-factor", label: "Free memory headroom", min: 0, max: 10000, usual: 125,
      help: "How much memory the kernel keeps free, in tenths of a percent. Larger values leave more room before it stalls." },
    { key: "watermark-boost-factor", label: "Memory boost", min: 0, max: 10000, usual: 0,
      help: "How hard the kernel reclaims after fragmentation. 0 turns it off." }
];

var PRIORITY_MIN = 0;
var PRIORITY_MAX = 32767;

/// Where a new swap file may go (XZram allows these and nothing else).
var SWAPFILE_PATHS = ["/swap/swapfile", "/swapfile", "/var/swap/swapfile"];

// ---------------------------------------------------------------------------------------------
// Argument patterns: command id -> argument -> pattern. tests/uq/logic.test.mjs checks that
// they are the same strings as page.toml's.

var VALUE = "^[A-Za-z0-9 ()+*/.,^%_][A-Za-z0-9 ()+*/.\\-_,^%]{0,199}$";
var ZKEY = "^(zram-size|zram-algorithm|zram-priority)$";
var TKEY = "^(" + TUNING.map(function (t) { return t.key; }).join("|") + ")$";
var TVAL = "^[0-9]{1,5}$";
var PATH = "^/[A-Za-z0-9/._-]{1,200}$";
var SIZE = "^[0-9]{1,7}$";
var SNAP = "^[A-Za-z0-9][A-Za-z0-9_.-]{0,79}$";
var KEEP = "^[0-9]{1,5}$";
var LABEL = "^[^\\x00-\\x1f\\x7f]{1,80}$";

var PATTERNS = {
    "plan-zram": { "key": ZKEY, "value": VALUE },
    "plan-zram-boot": { "key": ZKEY, "value": VALUE },
    "set-zram": { "key": ZKEY, "value": VALUE },
    "set-zram-boot": { "key": ZKEY, "value": VALUE },
    "plan-zram-off": {},
    "plan-zram-off-boot": {},
    "zram-off": {},
    "zram-off-boot": {},
    "plan-rollback": {},
    "rollback": {},
    "plan-tuning": { "key": TKEY, "value": TVAL },
    "set-tuning": { "key": TKEY, "value": TVAL },
    "plan-swapfile-create": { "size_mb": SIZE, "path": PATH },
    "swapfile-create": { "size_mb": SIZE, "path": PATH },
    "plan-swapfile-resize": { "size_mb": SIZE, "path": PATH },
    "swapfile-resize": { "size_mb": SIZE, "path": PATH },
    "plan-swapfile-remove": { "path": PATH },
    "swapfile-remove": { "path": PATH },
    "plan-snapshot-restore": { "id": SNAP },
    "snapshot-restore": { "id": SNAP },
    "plan-snapshot-delete": { "id": SNAP },
    "snapshot-delete": { "id": SNAP },
    "plan-snapshot-prune": { "keep": KEEP },
    "snapshot-prune": { "keep": KEEP },
    "snapshot-create": { "label": LABEL }
};

// ---------------------------------------------------------------------------------------------
// Small helpers

function isObject(v) {
    return v !== null && typeof v === "object" && !Array.isArray(v);
}

/// A string for display: control characters become spaces, and it is cut to `limit`.
function clip(value, limit) {
    var s = typeof value === "string" ? value : (value === null || value === undefined ? "" : String(value));
    s = s.replace(/[\u0000-\u001f\u007f]/g, " ");
    var n = limit || TEXT_LIMIT;
    return s.length > n ? s.slice(0, n - 1) + "…" : s;
}

function num(v) {
    return typeof v === "number" && isFinite(v) && v >= 0 ? v : 0;
}

function plural(n, one, many) {
    return n + " " + (n === 1 ? one : many);
}

function isCancelled(result) {
    return !!(result && result.cancelled);
}

/// "1.5 GiB": binary units, as the xzram command prints them.
function size(bytes) {
    var b = num(bytes);
    if (b < 1024)
        return Math.round(b) + " bytes";
    var units = ["KiB", "MiB", "GiB", "TiB"];
    var i = -1;
    do {
        b = b / 1024;
        i++;
    } while (b >= 1023.95 && i < units.length - 1);
    return b.toFixed(1) + " " + units[i];
}

/// How full a bar is, 0 to 1.
function fraction(used, total) {
    var t = num(total);
    if (t <= 0)
        return 0;
    return Math.max(0, Math.min(1, num(used) / t));
}

/// A command's output as an object, read for every exit code: `xzram doctor` exits 3 and
/// `xzram config set --json` exits 1, and both still print a document.
function documentOf(result) {
    var r = isObject(result) ? result : {};
    return isObject(r.output) || Array.isArray(r.output) ? r.output : null;
}

function failedRead(result, what) {
    var r = isObject(result) ? result : {};
    var doc = documentOf(result);
    var text = doc && typeof doc.error === "string" ? doc.error : (typeof r.error === "string" ? r.error : "");
    return { ok: false, error: text !== "" ? clip(text) : "Couldn't " + what + "." };
}

// ---------------------------------------------------------------------------------------------
// Reading the replies

/// `xzram status --json`: memory, pressure, swap areas and zram devices.
function parseStatus(result) {
    var d = documentOf(result);
    if (!isObject(d) || !isObject(d.memory))
        return failedRead(result, "read memory and swap");
    var m = d.memory;
    var total = num(m.mem_total_kb) * 1024;
    var available = num(m.mem_available_kb) * 1024;
    var swapTotal = num(m.swap_total_kb) * 1024;
    var swapFree = num(m.swap_free_kb) * 1024;
    var zram = [];
    var list = Array.isArray(d.zram_devices) ? d.zram_devices.slice(0, ROW_LIMIT) : [];
    for (var i = 0; i < list.length; ++i) {
        var z = list[i];
        if (!isObject(z))
            continue;
        var stored = num(z.data_bytes);
        var packed = num(z.compressed_bytes);
        zram.push({
            name: clip(z.name, 40),
            algorithm: clip(z.algorithm, 40),
            sizeBytes: num(z.disk_size_bytes),
            storedBytes: stored,
            compressedBytes: packed,
            ratio: stored > 0 && packed > 0 ? Math.round((stored / packed) * 10) / 10 : 0,
            active: z.mount_point === "[SWAP]"
        });
    }
    var swaps = [];
    var rows = Array.isArray(d.swaps) ? d.swaps.slice(0, ROW_LIMIT) : [];
    for (var j = 0; j < rows.length; ++j) {
        var s = rows[j];
        if (!isObject(s))
            continue;
        swaps.push({
            name: clip(s.name, 200),
            type: clip(s.swap_type, 40),
            sizeBytes: num(s.size_bytes),
            usedBytes: num(s.used_bytes),
            priority: typeof s.priority === "number" && isFinite(s.priority) ? Math.trunc(s.priority) : 0
        });
    }
    var psi = isObject(d.pressure) && isObject(d.pressure.memory) ? num(d.pressure.memory.some_avg60) : 0;
    return {
        ok: true,
        memory: {
            totalBytes: total,
            usedBytes: Math.max(0, total - available),
            swapTotalBytes: swapTotal,
            swapUsedBytes: Math.max(0, swapTotal - swapFree)
        },
        pressure: psi,
        zram: zram,
        swaps: swaps
    };
}

/// How hard memory is being pushed, from the share of time something waited for it.
function pressureText(psi) {
    var p = num(psi);
    if (p < 1)
        return "None. Nothing is waiting for memory.";
    if (p < 10)
        return "Some. Things wait for memory now and then.";
    return "High. Things are often waiting for memory.";
}

/// `xzram config show --json`.
function parseConfig(result) {
    var d = documentOf(result);
    if (!isObject(d) || !isObject(d.zram) || !isObject(d.tuning))
        return failedRead(result, "read the memory settings");
    var z = d.zram;
    function text(v) {
        return typeof v === "string" && v !== "" ? clip(v, 200) : null;
    }
    var tuning = {};
    for (var i = 0; i < TUNING.length; ++i) {
        var raw = d.tuning[TUNING[i].key.replace(/-/g, "_")];
        tuning[TUNING[i].key] = typeof raw === "number" && isFinite(raw) && raw >= 0 ? Math.trunc(raw) : null;
    }
    return {
        ok: true,
        zram: {
            configured: z.configured === true,
            size: text(z.size),
            algorithm: text(z.algorithm),
            priority: typeof z.priority === "number" && isFinite(z.priority) ? Math.trunc(z.priority) : null
        },
        tuning: tuning,
        pending: d.pending === true
    };
}

/// `xzram doctor --json`: {healthy, summary, issues}. Problems first.
function parseDoctor(result) {
    var d = documentOf(result);
    if (!isObject(d) || !Array.isArray(d.issues))
        return failedRead(result, "run the health check");
    var rank = { error: 0, warning: 1 };
    var issues = [];
    var list = d.issues.slice(0, ROW_LIMIT);
    for (var i = 0; i < list.length; ++i) {
        var it = list[i];
        if (!isObject(it) || typeof it.message !== "string")
            continue;
        var severity = it.severity === "error" ? "error" : (it.severity === "warning" ? "warning" : "note");
        issues.push({
            severity: severity,
            message: clip(it.message),
            suggestion: typeof it.suggestion === "string" ? clip(it.suggestion) : ""
        });
    }
    issues.sort(function (a, b) {
        var ra = rank[a.severity] === undefined ? 2 : rank[a.severity];
        var rb = rank[b.severity] === undefined ? 2 : rank[b.severity];
        return ra - rb;
    });
    return {
        ok: true,
        healthy: d.healthy === true,
        summary: typeof d.summary === "string" ? clip(d.summary, 120) : (d.healthy === true ? "Healthy" : "Needs attention"),
        issues: issues
    };
}

/// `xzram swapfile list --json`: the swap files in /etc/fstab.
function parseSwapfiles(result) {
    var d = documentOf(result);
    if (!Array.isArray(d))
        return failedRead(result, "read the swap files");
    var files = [];
    var list = d.slice(0, ROW_LIMIT);
    for (var i = 0; i < list.length; ++i) {
        var f = list[i];
        if (!isObject(f) || typeof f.path !== "string" || !new RegExp(PATH).test(f.path))
            continue;
        files.push({
            path: f.path,
            sizeMb: Math.trunc(num(f.size_mb)),
            priority: typeof f.priority === "number" && isFinite(f.priority) ? Math.trunc(f.priority) : 0
        });
    }
    return { ok: true, files: files };
}

/// `xzram snapshot list --json`, newest first.
function parseSnapshots(result) {
    var d = documentOf(result);
    if (!Array.isArray(d))
        return failedRead(result, "read the snapshots");
    var rows = [];
    var list = d.slice(0, ROW_LIMIT);
    for (var i = 0; i < list.length; ++i) {
        var s = list[i];
        if (!isObject(s) || typeof s.id !== "string" || !new RegExp(SNAP).test(s.id))
            continue;
        rows.push({
            id: s.id,
            label: clip(s.label, 120),
            when: whenText(s.created_at),
            trigger: s.trigger === "pre_apply" ? "Before a change" : (s.trigger === "app_open" ? "When XZram opened" : "Taken by hand"),
            before: s.trigger === "pre_apply"
        });
    }
    return { ok: true, snapshots: rows };
}

/// "2026-11-16 10:30" from an RFC 3339 time; the text itself when it is another shape.
function whenText(value) {
    var m = /^(\d{4}-\d{2}-\d{2})T(\d{2}:\d{2})/.exec(typeof value === "string" ? value : "");
    return m ? m[1] + " " + m[2] : clip(value, 40);
}

// ---------------------------------------------------------------------------------------------
// Choices

function squash(expr) {
    return String(expr).replace(/\s+/g, "").toLowerCase();
}

/// The rows of the size menu: `{value, label}`. A size that matches no preset (or none set at
/// all) comes first, so the menu always shows what is in force.
function sizeChoices(current) {
    var rows = [];
    var found = false;
    var i;
    for (i = 0; i < SIZE_PRESETS.length; ++i)
        if (current && squash(SIZE_PRESETS[i].value) === squash(current))
            found = true;
    if (!current)
        rows.push({ value: "", label: "Default" });
    else if (!found)
        rows.push({ value: current, label: "Custom: " + clip(current, 60) });
    for (i = 0; i < SIZE_PRESETS.length; ++i)
        rows.push({ value: SIZE_PRESETS[i].value, label: SIZE_PRESETS[i].label });
    return rows;
}

function sizeIndex(current) {
    var rows = sizeChoices(current);
    for (var i = 0; i < rows.length; ++i)
        if (rows[i].value === (current || "") || (current && squash(rows[i].value) === squash(current)))
            return i;
    return 0;
}

/// The names in the algorithm menu: the usual ones, plus the one in force if it's another.
function algorithmChoices(current) {
    var names = ALGORITHMS.slice();
    if (current && names.indexOf(current) < 0 && /^[A-Za-z0-9_][A-Za-z0-9_-]{0,63}$/.test(current))
        names.unshift(current);
    if (!current)
        names.unshift("Default");
    return names;
}

function algorithmIndex(current) {
    var i = algorithmChoices(current).indexOf(current || "Default");
    return i < 0 ? 0 : i;
}

function tuningDef(key) {
    for (var i = 0; i < TUNING.length; ++i)
        if (TUNING[i].key === key)
            return TUNING[i];
    return null;
}

/// "" when `text` is a whole number in the range of the tuning `key`, else the reason.
function checkTuning(key, text) {
    var def = tuningDef(key);
    if (!def)
        return "That setting isn't known.";
    var t = String(text).trim();
    if (!/^[0-9]{1,5}$/.test(t) || Number(t) < def.min || Number(t) > def.max)
        return def.label + " is a whole number from " + def.min + " to " + def.max + ".";
    return "";
}

function checkPriority(text) {
    var t = String(text).trim();
    if (!/^[0-9]{1,5}$/.test(t) || Number(t) < PRIORITY_MIN || Number(t) > PRIORITY_MAX)
        return "Priority is a whole number from " + PRIORITY_MIN + " to " + PRIORITY_MAX + ".";
    return "";
}

/// "" when a new or resized swap file is acceptable, else the reason. `gb` is text.
function checkSwapfile(path, gb) {
    if (!new RegExp(PATH).test(path) || (SWAPFILE_PATHS.indexOf(path) < 0 && path.indexOf("/swap/") !== 0
            && path.indexOf("/var/swap/") !== 0 && path.indexOf("/var/lib/swap/") !== 0))
        return "A swap file goes in /swap/, /var/swap/, /var/lib/swap/, or is exactly /swapfile.";
    return checkSwapSize(gb);
}

function checkSwapSize(gb) {
    var t = String(gb).trim();
    if (!/^[0-9]{1,4}(\.[0-9]{1,2})?$/.test(t) || Number(t) < 0.25 || Number(t) > 1024)
        return "Choose a size from 0.25 to 1024 GB.";
    return "";
}

/// The size in MiB for the swap file commands, from GiB text.
function mibOf(gb) {
    return String(Math.round(Number(String(gb).trim()) * 1024));
}

/// "" when the arguments match the command's patterns in page.toml, else a sentence. The same
/// check uQ makes, so the page can word it instead of getting a bare refusal.
function argsProblem(command, args) {
    var table = PATTERNS[command];
    if (!table)
        return "That isn't something this page can do.";
    var names = Object.keys(table);
    if (!Array.isArray(args) || args.length !== names.length)
        return "That change was missing a value.";
    for (var i = 0; i < names.length; ++i) {
        var a = args[i];
        if (typeof a !== "string" || a.charAt(0) === "-" || !new RegExp(table[names[i]]).test(a))
            return "That value isn't one XZram accepts.";
    }
    return "";
}
