.pragma library
.import "logic.js" as Logic
// Plans and progress for the Memory and swap page: a previewed plan, a streamed change's
// progress, and the note it leaves. No Qt calls, so Node can test it (tests/uq/logic.test.mjs).
// Reading and checking values is in logic.js.

var isObject = Logic.isObject;
var clip = Logic.clip;
var documentOf = Logic.documentOf;
var failedRead = Logic.failedRead;
var tuningDef = Logic.tuningDef;

/// A previewed plan from a `plan-*` command: {ok, refuse, reason, memory}. `memory` is true
/// when it would be refused for lack of free memory, the one refusal with a way forward
/// (saving for the next restart).
function parsePlan(result) {
    var d = documentOf(result);
    if (!isObject(d) || typeof d.would_refuse !== "boolean")
        return failedRead(result, "check what that would do");
    var reason = typeof d.reason === "string" ? clip(d.reason, 400) : "";
    return {
        ok: true,
        refuse: d.would_refuse,
        reason: reason,
        memory: d.would_refuse && reason.toLowerCase().indexOf(Logic.MEMORY_REFUSAL) >= 0
    };
}

var LOW_MEMORY = "Not changed. There isn't enough free memory right now to move what's in swap back. Try again with fewer apps open, or save it for the next restart.";

/// What the page shows for a refused plan.
function refusalText(plan) {
    if (plan.memory)
        return LOW_MEMORY;
    return plan.reason !== "" ? "Not changed. " + plan.reason : "Not changed.";
}

/// One event from a streamed command as the progress bar shows it:
/// {label, percent (-1 when unknown), detail}, or null for an event that isn't progress.
function progressOf(ev) {
    if (!isObject(ev) || (ev.event !== "start" && ev.event !== "progress"))
        return null;
    var p = typeof ev.percent === "number" && isFinite(ev.percent) ? Math.max(0, Math.min(99, Math.round(ev.percent))) : -1;
    return {
        label: clip(ev.label, 120) || "Working…",
        percent: p,
        detail: ev.event === "progress" ? clip(ev.detail, 120) : ""
    };
}

/// The note under a row once a streamed write is over. `last` is the last `done` or `error`
/// event the stream sent (or null), `result` what `onDone` got, `done` the sentence for success.
/// {message, failed, kind, detail}.
function outcome(last, result, done) {
    if (isObject(last) && last.event === "done" && last.ok === true)
        return { message: done, failed: false, kind: "done", detail: "" };
    if (isObject(last) && last.event === "error") {
        var raw = clip(last.error, 400);
        return failure(typeof last.kind === "string" ? last.kind : "other", raw);
    }
    // No final event: the command died, was stopped, or timed out.
    var r = isObject(result) ? result : {};
    if (r.code === 0)
        return { message: done, failed: false, kind: "done", detail: "" };
    var text = clip(r.error, 400);
    return failure(timedOut(text) ? "pending" : "other", text);
}

function timedOut(text) {
    var low = text.toLowerCase();
    return low.indexOf("timed out") >= 0 || low.indexOf("timeout") >= 0;
}

function failure(kind, raw) {
    if (kind === "memory")
        return { message: LOW_MEMORY, failed: true, kind: "memory", detail: raw };
    if (kind === "denied")
        return { message: "Not changed. Administrator permission wasn't given.", failed: true, kind: "denied", detail: raw };
    if (kind === "pending")
        return { message: "Still working. Check again in a moment.", failed: false, kind: "pending", detail: raw };
    return { message: "Couldn't make that change.", failed: true, kind: "other", detail: raw };
}

/// A note for a change refused before it was sent (a bad value, a refused plan).
function problem(message) {
    return { message: message, failed: true, kind: "other", detail: "" };
}

/// A copy of `notes` with `key` set to `note`, or removed when `note` is null.
function withNote(notes, key, note) {
    var out = {};
    for (var k in notes)
        if (Object.prototype.hasOwnProperty.call(notes, k) && k !== key)
            out[k] = notes[k];
    if (note)
        out[key] = note;
    return out;
}

/// The wording of what a finished change did.
function doneText(key, value) {
    if (key === "zram-size")
        return "Compressed memory size changed.";
    if (key === "zram-algorithm")
        return "Compression changed.";
    if (key === "zram-priority")
        return "Priority changed.";
    var def = tuningDef(key);
    return (def ? def.label : "Setting") + " set to " + value + ".";
}
