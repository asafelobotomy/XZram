.pragma library
// Pure logic for the Memory and swap page: parsing XZram's replies, checking values before
// they are sent, building the two write payloads and wording errors. No Qt calls, so Node can
// load it (tests/uq/logic.test.mjs). Everything that comes from a reply is untrusted: wrong
// types give a "couldn't read" result, never an exception.

var MAX_ROWS = 500;
var MAX_TEXT = 200;
var SWAPPINESS_MIN = 0;
var SWAPPINESS_MAX = 200;

/// The same character rule XZram's own validation applies to a zram size expression
/// (letters, digits and " ()+*/.-_,^%"); "]" and newlines are not in it.
var SIZE_PATTERN = /^[A-Za-z0-9 ()+*\/.\-_,^%]{1,200}$/;
/// A compression algorithm name: one token, never starting with a dash.
var ALGORITHM_PATTERN = /^[A-Za-z0-9_][A-Za-z0-9_-]{0,63}$/;
var DEVICE_PATTERN = /^zram[0-9]+$/;

/// Sizes the page offers: zram-generator expressions in terms of installed memory.
var SIZE_PRESETS = [
    { value: "ram / 4", label: "A quarter of memory" },
    { value: "ram / 2", label: "Half of memory" },
    { value: "ram * 3 / 4", label: "Three quarters of memory" },
    { value: "ram", label: "As much as memory" }
];

/// Algorithms the page offers (the kernel's usual names). The current one is added if missing.
var ALGORITHMS = ["zstd", "lz4", "lzo-rle", "lzo", "842"];

/// What the person is asked before a change that moves swap in use (the exact texts).
var CONFIRMS = {
    "zram-size": "Change the zram size? Swap in use is moved first; this can take a while.",
    "zram-algorithm": "Change the zram compression? Swap in use is moved first; this can take a while."
};

var STILL_WORKING = "Still working. Check again in a moment.";

/// XZram refuses to restart compressed memory when RAM can't take back what is stored in it
/// (its refusal always contains this phrase). The page then offers to apply at next restart.
var MEMORY_REFUSAL = "not enough free memory";
var LOW_MEMORY = "Not changed. There isn't enough free memory right now to move what's in compressed memory back. Try again with fewer apps open, or apply it at the next restart.";
var AT_BOOT_DONE = "Saved. It takes effect at the next restart.";

function clip(value) {
    var s = String(value === undefined || value === null ? "" : value);
    return s.length > MAX_TEXT ? s.slice(0, MAX_TEXT) + "…" : s;
}

function isObject(v) {
    return v !== null && typeof v === "object" && !Array.isArray(v);
}

function num(v) {
    return typeof v === "number" && isFinite(v) && v >= 0 ? v : 0;
}

// ---------------------------------------------------------------------------------------------
// Replies

/// XZram answers every read with `a{sv}` holding one `json` string. Returns
/// `{ok: true, data}` or `{ok: false, error}`.
function unwrap(reply) {
    if (!isObject(reply))
        return { ok: false, error: "No answer." };
    if (reply.error)
        return { ok: false, error: clip(reply.error) };
    var v = reply.value;
    if (!isObject(v) || typeof v.json !== "string")
        return { ok: false, error: "The answer was not in the expected form." };
    try {
        return { ok: true, data: JSON.parse(v.json) };
    } catch (e) {
        return { ok: false, error: "The answer could not be read." };
    }
}

/// GetStatus: memory totals, active swap and zram devices.
function parseStatus(reply) {
    var r = unwrap(reply);
    if (!r.ok)
        return r;
    var d = r.data;
    if (!isObject(d) || !isObject(d.memory))
        return { ok: false, error: "The memory status was not in the expected form." };
    var m = d.memory;
    var devices = [];
    var list = Array.isArray(d.zram_devices) ? d.zram_devices.slice(0, MAX_ROWS) : [];
    for (var i = 0; i < list.length; ++i) {
        var z = list[i];
        if (!isObject(z))
            continue;
        devices.push({
            name: clip(z.name),
            algorithm: clip(z.algorithm),
            sizeBytes: num(z.disk_size_bytes),
            dataBytes: num(z.data_bytes),
            compressedBytes: num(z.compressed_bytes)
        });
    }
    return {
        ok: true,
        memory: {
            totalBytes: num(m.mem_total_kb) * 1024,
            availableBytes: num(m.mem_available_kb) * 1024,
            swapTotalBytes: num(m.swap_total_kb) * 1024,
            swapFreeBytes: num(m.swap_free_kb) * 1024
        },
        zram: devices
    };
}

/// GetZramConfig: the zram device configuration, or `null` when none is configured.
function parseZramConfig(reply) {
    var r = unwrap(reply);
    if (!r.ok)
        return r;
    if (r.data === null)
        return { ok: true, config: null };
    var c = r.data;
    if (!isObject(c) || !DEVICE_PATTERN.test(String(c.device)))
        return { ok: false, error: "The zram settings were not in the expected form." };
    function optString(v) {
        return typeof v === "string" ? v : null;
    }
    return {
        ok: true,
        config: {
            device: c.device,
            zram_size: optString(c.zram_size),
            zram_resident_limit: optString(c.zram_resident_limit),
            compression_algorithm: optString(c.compression_algorithm),
            swap_priority: typeof c.swap_priority === "number" && isFinite(c.swap_priority)
                ? Math.trunc(c.swap_priority) : null,
            fs_type: optString(c.fs_type),
            mount_point: optString(c.mount_point)
        }
    };
}

/// GetSysctl: the current swappiness (`null` when the kernel didn't report it).
function parseSysctl(reply) {
    var r = unwrap(reply);
    if (!r.ok)
        return r;
    if (!isObject(r.data))
        return { ok: false, error: "The memory tuning was not in the expected form." };
    var s = r.data.swappiness;
    var ok = typeof s === "number" && isFinite(s) && s >= SWAPPINESS_MIN && s <= SWAPPINESS_MAX;
    return { ok: true, swappiness: ok ? Math.trunc(s) : null };
}

/// ListSwaps: every swap area, active or only configured.
function parseSwaps(reply) {
    var r = unwrap(reply);
    if (!r.ok)
        return r;
    if (!Array.isArray(r.data))
        return { ok: false, error: "The swap list was not in the expected form." };
    var rows = [];
    var list = r.data.slice(0, MAX_ROWS);
    for (var i = 0; i < list.length; ++i) {
        var s = list[i];
        if (!isObject(s))
            continue;
        rows.push({
            name: clip(s.name),
            type: clip(s.swap_type),
            sizeBytes: num(s.size_bytes),
            usedBytes: num(s.used_bytes),
            active: s.active === true
        });
    }
    return { ok: true, swaps: rows };
}

// ---------------------------------------------------------------------------------------------
// Wording

function formatBytes(bytes) {
    var b = num(bytes);
    var units = ["bytes", "KB", "MB", "GB", "TB"];
    var i = 0;
    while (b >= 1024 && i < units.length - 1) {
        b /= 1024;
        ++i;
    }
    if (i === 0)
        return Math.round(b) + " bytes";
    return (Math.round(b * 10) / 10) + " " + units[i];
}

/// The lines under "Memory": memory in use, swap in use and each compressed-memory device.
function statusLines(status) {
    if (!status)
        return [];
    var m = status.memory;
    var lines = [];
    var usedMem = Math.max(0, m.totalBytes - m.availableBytes);
    lines.push("Memory: " + formatBytes(usedMem) + " in use of " + formatBytes(m.totalBytes));
    if (m.swapTotalBytes === 0) {
        lines.push("Swap: none in use");
    } else {
        var usedSwap = Math.max(0, m.swapTotalBytes - m.swapFreeBytes);
        lines.push("Swap: " + formatBytes(usedSwap) + " in use of " + formatBytes(m.swapTotalBytes));
    }
    for (var i = 0; i < status.zram.length; ++i) {
        var z = status.zram[i];
        var line = z.name + ": " + (z.algorithm || "unknown") + ", " + formatBytes(z.sizeBytes);
        if (z.compressedBytes > 0 && z.dataBytes > 0) {
            var ratio = Math.round((z.dataBytes / z.compressedBytes) * 10) / 10;
            line += ", " + formatBytes(z.dataBytes) + " stored in " + formatBytes(z.compressedBytes) + " (" + ratio + " to 1)";
        }
        lines.push(line);
    }
    return lines;
}

/// One line for a row of the swap list.
function swapLine(swap) {
    var kind = swap.type ? swap.type + ", " : "";
    if (!swap.active)
        return swap.name + ": " + kind + "not in use right now";
    return swap.name + ": " + kind + formatBytes(swap.usedBytes) + " used of " + formatBytes(swap.sizeBytes);
}

// ---------------------------------------------------------------------------------------------
// Choices

function squash(expr) {
    return String(expr).replace(/\s+/g, "").toLowerCase();
}

/// The rows of the size menu: `{value, label}`. A size that matches no preset (or none set at
/// all) is shown first, so the menu always shows what is in force. `value` is "" for "not set".
function sizeChoices(current) {
    var rows = [];
    var found = false;
    var i;
    for (i = 0; i < SIZE_PRESETS.length; ++i)
        if (current && squash(SIZE_PRESETS[i].value) === squash(current))
            found = true;
    if (current === null || current === undefined || current === "")
        rows.push({ value: "", label: "Default" });
    else if (!found)
        rows.push({ value: current, label: "Custom: " + clip(current) });
    for (i = 0; i < SIZE_PRESETS.length; ++i)
        rows.push({ value: SIZE_PRESETS[i].value, label: SIZE_PRESETS[i].label });
    return rows;
}

/// Index of the size in force within `sizeChoices(current)`.
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
    if (current && names.indexOf(current) < 0 && ALGORITHM_PATTERN.test(current))
        names.unshift(current);
    if (!current)
        names.unshift("Default");
    return names;
}

function algorithmIndex(current) {
    var names = algorithmChoices(current);
    var i = names.indexOf(current || "Default");
    return i < 0 ? 0 : i;
}

// ---------------------------------------------------------------------------------------------
// Checks and payloads

/// "" when `expr` is an acceptable zram size, else the reason.
function checkSize(expr) {
    if (typeof expr !== "string" || expr.trim() === "")
        return "Choose a size.";
    if (expr.length > 200)
        return "That size is too long.";
    if (!SIZE_PATTERN.test(expr))
        return "That size has characters XZram doesn't accept.";
    return "";
}

function checkAlgorithm(name) {
    if (typeof name !== "string" || !ALGORITHM_PATTERN.test(name))
        return "That compression name isn't valid.";
    return "";
}

function checkSwappiness(value) {
    var n = typeof value === "string" && /^[0-9]{1,3}$/.test(value) ? Number(value) : value;
    if (typeof n !== "number" || !isFinite(n) || Math.floor(n) !== n || n < SWAPPINESS_MIN || n > SWAPPINESS_MAX)
        return "Swappiness is a whole number from " + SWAPPINESS_MIN + " to " + SWAPPINESS_MAX + ".";
    return "";
}

/// The `ApplyNowZram` argument: the whole current configuration with one field changed
/// (`changes` is `{size}` or `{algorithm}`). `{ok: true, json, kind}` or `{ok: false, error}`.
function buildZramConfig(current, changes) {
    if (!current || !DEVICE_PATTERN.test(String(current.device)))
        return { ok: false, error: "The current zram settings couldn't be read, so nothing was changed." };
    var out = {
        device: current.device,
        zram_size: current.zram_size,
        zram_resident_limit: current.zram_resident_limit,
        compression_algorithm: current.compression_algorithm,
        swap_priority: current.swap_priority,
        fs_type: current.fs_type,
        mount_point: current.mount_point
    };
    var kind;
    var problem;
    if (changes && changes.size !== undefined) {
        problem = checkSize(changes.size);
        out.zram_size = String(changes.size).trim();
        kind = "zram-size";
    } else if (changes && changes.algorithm !== undefined) {
        problem = checkAlgorithm(changes.algorithm);
        out.compression_algorithm = changes.algorithm;
        kind = "zram-algorithm";
    } else {
        return { ok: false, error: "Nothing to change." };
    }
    if (problem !== "")
        return { ok: false, error: problem };
    return { ok: true, json: JSON.stringify(out), kind: kind };
}

/// The `ApplyNowSysctl` argument: only swappiness, which XZram merges with what's there.
function buildSysctl(swappiness) {
    var problem = checkSwappiness(swappiness);
    if (problem !== "")
        return { ok: false, error: problem };
    return { ok: true, json: JSON.stringify({ swappiness: Number(swappiness) }) };
}

// ---------------------------------------------------------------------------------------------
// Errors

/// Sorts an `{error}` from a call into a kind and a plain sentence:
/// `unavailable` (the service didn't answer), `denied`, `pending` (a deadline passed, the work
/// may still be going), `memory` (not enough free memory to restart compressed memory now),
/// `refused` (XZram rejected the value) or `other`.
/// `what` finishes "Couldn't …", for example "change the zram size".
function describeError(error, what) {
    var raw = clip(error);
    var low = raw.toLowerCase();
    if (String(error === undefined || error === null ? "" : error).toLowerCase().indexOf(MEMORY_REFUSAL) >= 0)
        return { kind: "memory", message: LOW_MEMORY, raw: raw };
    if (low.indexOf("no answer within") >= 0 || low.indexOf("timeout") >= 0 || low.indexOf("timed out") >= 0 || low.indexOf("deadline") >= 0)
        return { kind: "pending", message: STILL_WORKING, raw: raw };
    if (low.indexOf("is not running") >= 0 || low.indexOf("serviceunknown") >= 0 || low.indexOf("was not provided") >= 0 || low.indexOf("could not be activated") >= 0 || low.indexOf("activation") >= 0 || low.indexOf("no such name") >= 0)
        return { kind: "unavailable", message: "XZram isn't answering. It starts when needed, so try again in a moment.", raw: raw };
    if (low.indexOf("accessdenied") >= 0 || low.indexOf("not authorized") >= 0 || low.indexOf("not authorised") >= 0 || low.indexOf("authorization") >= 0 || low.indexOf("permission") >= 0 || low.indexOf("access denied") >= 0)
        return { kind: "denied", message: "Not changed. Administrator permission wasn't given.", raw: raw };
    if (low.indexOf("invalidargs") >= 0 || low.indexOf("invalid args") >= 0 || low.indexOf("unsupported field") >= 0)
        return { kind: "refused", message: "XZram didn't accept that value, so nothing was changed.", raw: raw };
    return { kind: "other", message: "Couldn't " + what + ".", raw: raw };
}

/// True for an error worth one more try on the first read (the service was still starting).
function isRetryable(error) {
    var kind = describeError(error, "read").kind;
    return kind === "pending" || kind === "unavailable";
}

/// Wraps a callback so it does nothing once `life.gone` is true. The wrapper lives here, not
/// in the page, so calling it after the page is destroyed is safe (a function made inside a
/// destroyed page can't be called at all).
function guarded(life, fn) {
    return function () {
        if (life.gone)
            return undefined;
        return fn.apply(null, arguments);
    };
}
