// Memory and swap: what XZram shows and everything it changes. Everything goes through the
// xzram command line (page.toml); logic.js reads its answers and words the result. Changes
// show what they will do and ask first (uQ's confirmation sheet), stream their progress, and
// are never forced: XZram's own refusals are shown as they are.
// Laid out like Settings' own pages (uq_api = 3): five tabs of cards.
import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.uq.settings
import "logic.js" as Logic
import "plans.js" as Plans

SettingsPage {
    id: page

    property var ctx
    property var theme
    /// The [[setting]] row Settings search jumped to; the rows flash, and their tab opens.
    property string highlight

    // What was last read (null until it has been) and why a read failed ("" when it didn't).
    property var status: null
    property var config: null
    property var doctor: null
    property var swapfiles: null
    property var snapshots: null
    property string statusProblem: ""
    property string configProblem: ""
    property string doctorProblem: ""
    property string swapfilesProblem: ""
    property string snapshotsProblem: ""
    property bool doctorLoading: false
    property bool statusLoading: false
    property bool moreAvailable: true

    /// The key of the row whose change is running, "" when none. One change runs at a time.
    property string workingKey: ""
    readonly property bool busy: page.workingKey !== ""
    /// What the running change is doing: {label, percent (-1 when unknown), detail}.
    property var progress: null
    /// What each change said, by row key: {message, failed, kind, detail}.
    property var notes: ({})
    /// A refused change that can be saved for the next restart: {key, command, args, done}.
    property var offer: null
    property var openedWith: null
    /// Set when the page is destroyed: a reply that comes after that does nothing.
    readonly property var life: ({ gone: false })
    Component.onDestruction: page.life.gone = true

    pageId: "xzram"

    function noteFor(key) {
        return page.notes[key] || null;
    }

    function errorColor() {
        const colors = page.ctx && page.ctx.theme ? page.ctx.theme.colors : null;
        return (colors && colors.error) || "red";
    }

    // -----------------------------------------------------------------------------------------
    // Reading

    function readStatus() {
        if (!page.ctx || page.statusLoading)
            return;
        page.statusLoading = true;
        page.ctx.command.run("status", [], result => {
            if (page.life.gone)
                return;
            page.statusLoading = false;
            const read = Logic.parseStatus(result);
            if (read.ok) {
                page.status = read;
                page.statusProblem = "";
            } else {
                page.statusProblem = read.error;
            }
        });
    }

    function takeConfig(result) {
        if (page.life.gone)
            return;
        const read = Logic.parseConfig(result);
        if (read.ok) {
            page.config = read;
            page.configProblem = "";
        } else {
            page.configProblem = read.error;
        }
    }

    function takeSwapfiles(result) {
        if (page.life.gone)
            return;
        const read = Logic.parseSwapfiles(result);
        if (read.ok) {
            page.swapfiles = read;
            page.swapfilesProblem = "";
        } else {
            page.swapfilesProblem = read.error;
        }
    }

    function readSnapshots() {
        if (!page.ctx)
            return;
        page.ctx.command.run("snapshots", [], result => {
            if (page.life.gone)
                return;
            const read = Logic.parseSnapshots(result);
            if (read.ok) {
                page.snapshots = read;
                page.snapshotsProblem = "";
            } else {
                page.snapshotsProblem = read.error;
            }
        });
    }

    function readDoctor() {
        if (!page.ctx || page.doctorLoading)
            return;
        page.doctorLoading = true;
        page.ctx.command.run("doctor", [], result => {
            if (page.life.gone)
                return;
            page.doctorLoading = false;
            const read = Logic.parseDoctor(result);
            if (read.ok) {
                page.doctor = read;
                page.doctorProblem = "";
            } else {
                page.doctorProblem = read.error;
            }
        });
    }

    /// After a change: everything it may have moved. The watched reads follow by themselves.
    function refreshAll() {
        page.readStatus();
        page.readSnapshots();
        page.readDoctor();
    }

    // The cheap reads now; the config and the swap files are followed from then on.
    onCtxChanged: {
        if (page.ctx && page.ctx !== page.openedWith) {
            page.openedWith = page.ctx;
            page.readStatus();
            page.ctx.command.watch("config", page.takeConfig);
            page.ctx.command.watch("swapfiles", page.takeSwapfiles);
            page.readSnapshots();
            page.readDoctor();
        }
    }

    // Memory moves all the time: while a tab that shows it is open, read it every few seconds.
    Timer {
        interval: 5000
        repeat: true
        running: !!page.ctx && page.visible && !page.busy && (page.currentIndex === 0 || page.currentIndex === 3)
        onTriggered: page.readStatus()
    }

    // -----------------------------------------------------------------------------------------
    // Changing

    function setNote(key, note) {
        page.notes = Plans.withNote(page.notes, key, note);
    }

    /// Checks the arguments, then starts the row `key`. False (and a note) when they're wrong.
    function begin(key, command, args) {
        if (page.busy || !page.ctx)
            return false;
        const why = Logic.argsProblem(command, args);
        page.offer = null;
        page.setNote(key, why === "" ? null : Plans.problem(why));
        return why === "";
    }

    /// Streams the write `command` for the row `key` (uQ asks first and shows the plan), then
    /// notes what it said. A cancel changes nothing and shows nothing.
    function stream(key, command, args, doneText, after) {
        if (!page.begin(key, command, args)) {
            if (after)
                after();
            return;
        }
        let last = null;
        page.workingKey = key;
        page.progress = { label: "Starting…", percent: -1, detail: "" };
        page.ctx.command.stream(command, args, ev => {
            if (page.life.gone)
                return;
            const shown = Plans.progressOf(ev);
            if (shown)
                page.progress = shown;
            else if (ev && (ev.event === "done" || ev.event === "error"))
                last = ev;
        }, result => {
            if (page.life.gone)
                return;
            page.workingKey = "";
            if (!Logic.isCancelled(result)) {
                page.setNote(key, Plans.outcome(last, result, doneText));
                page.refreshAll();
            }
            if (after)
                after();
        });
    }

    /// Asks XZram what a change would do before asking the person: a change it would refuse for
    /// lack of memory says so here, with the offer in `offer`; anything else goes on with `go`.
    function checked(key, planCommand, args, offer, go, after) {
        if (!page.begin(key, planCommand, args)) {
            if (after)
                after();
            return;
        }
        page.workingKey = key;
        page.progress = { label: "Checking what that would do…", percent: -1, detail: "" };
        page.ctx.command.run(planCommand, args, result => {
            if (page.life.gone)
                return;
            page.workingKey = "";
            const plan = Plans.parsePlan(result);
            if (!plan.ok) {
                page.setNote(key, Plans.problem(plan.error));
            } else if (plan.refuse) {
                page.setNote(key, { message: Plans.refusalText(plan), failed: true, kind: plan.memory ? "memory" : "other", detail: plan.reason });
                if (plan.memory && offer)
                    page.offer = { key: key, command: offer.command, args: offer.args, done: offer.done };
            } else {
                go();
                return;
            }
            if (after)
                after();
        });
    }

    /// After a refusal for lack of memory: save the same change for the next restart.
    function takeOffer() {
        const o = page.offer;
        if (!o)
            return;
        page.stream(o.key, o.command, o.args, o.done);
    }

    readonly property string savedForBoot: "Saved. It takes effect at the next restart."

    function changeZram(key, value, after) {
        const args = [key, value];
        page.checked(key, "plan-zram", args,
            { command: "set-zram-boot", args: args, done: page.savedForBoot },
            () => page.stream(key, "set-zram", args, Plans.doneText(key, value), after), after);
    }

    function changeTuning(key, value, after) {
        const why = Logic.checkTuning(key, value);
        if (why !== "") {
            page.setNote(key, Plans.problem(why));
            if (after)
                after();
            return;
        }
        page.stream(key, "set-tuning", [key, String(value).trim()], Plans.doneText(key, String(value).trim()), after);
    }

    function turnZramOff() {
        page.checked("zram-off", "plan-zram-off", [],
            { command: "zram-off-boot", args: [], done: page.savedForBoot },
            () => page.stream("zram-off", "zram-off", [], "Compressed memory is off."));
    }

    /// False (and a note) when the path or size is wrong.
    function createSwapfile(path, gb) {
        const why = Logic.checkSwapfile(path, gb);
        if (why !== "") {
            page.setNote("swapfile-new", Plans.problem(why));
            return false;
        }
        const args = [Logic.mibOf(gb), path];
        page.checked("swapfile-new", "plan-swapfile-create", args, null,
            () => page.stream("swapfile-new", "swapfile-create", args, "Swap file created."));
        return true;
    }

    function resizeSwapfile(path, gb) {
        const key = "swapfile:" + path;
        const why = Logic.checkSwapSize(gb);
        if (why !== "") {
            page.setNote(key, Plans.problem(why));
            return false;
        }
        const args = [Logic.mibOf(gb), path];
        page.checked(key, "plan-swapfile-resize", args, null,
            () => page.stream(key, "swapfile-resize", args, "Swap file resized."));
        return true;
    }

    function removeSwapfile(path) {
        const key = "swapfile:" + path;
        page.checked(key, "plan-swapfile-remove", [path], null,
            () => page.stream(key, "swapfile-remove", [path], "Swap file removed."));
    }

    function restoreSnapshot(id) {
        const key = "snapshot:" + id;
        page.checked(key, "plan-snapshot-restore", [id], null,
            () => page.stream(key, "snapshot-restore", [id], "Snapshot restored."));
    }

    function deleteSnapshot(id) {
        page.stream("snapshot:" + id, "snapshot-delete", [id], "Snapshot deleted.");
    }

    function pruneSnapshots(keep) {
        page.stream("snapshot-prune", "snapshot-prune", [String(keep)], "Older snapshots deleted.");
    }

    function rollBack() {
        page.checked("rollback", "plan-rollback", [], null,
            () => page.stream("rollback", "rollback", [], "Went back to before the last change."));
    }

    function takeSnapshot() {
        if (!page.begin("snapshot-new", "snapshot-create", ["Taken from Settings"]))
            return;
        page.workingKey = "snapshot-new";
        page.progress = { label: "Taking a snapshot…", percent: -1, detail: "" };
        page.ctx.command.run("snapshot-create", ["Taken from Settings"], result => {
            if (page.life.gone)
                return;
            page.workingKey = "";
            const ok = !!result && result.code === 0;
            page.setNote("snapshot-new", ok
                ? { message: "Snapshot taken.", failed: false, kind: "done", detail: "" }
                : Plans.failure("other", Logic.clip(result ? (result.error || "") : "", 400)));
            page.readSnapshots();
        });
    }

    function openXzram() {
        if (!page.ctx)
            return;
        page.ctx.openApp("io.github.XZram.desktop", reply => {
            if (page.life.gone)
                return;
            if (reply && reply.error)
                page.moreAvailable = false;
        });
    }

    headerItems: [
        QQC2.Button {
            objectName: "moreButton"
            visible: page.moreAvailable
            text: "More in XZram"
            flat: true
            onClicked: page.openXzram()
        }
    ]

    banners: [
        ErrorCard {
            objectName: "configError"
            title: "Couldn't read the memory settings"
            detail: page.configProblem
            visible: detail !== ""
            Layout.fillWidth: true
        }
    ]

    OverviewTab {
        host: page
    }

    SettingsTab {
        title: "Compressed memory"
        iconName: "package-x-generic"

        SettingsCard {
            title: "Size and compression"
            iconName: "media-flash"
            help: "Changing these moves what is in swap back into memory first, so XZram checks there is room."
            wide: true
            Layout.fillWidth: true

            ZramSection {
                host: page
                Layout.fillWidth: true
            }
        }
    }

    SettingsTab {
        title: "Tuning"
        iconName: "configure"

        SettingsCard {
            title: "Kernel memory settings"
            iconName: "configure"
            help: "Kept after restarts. Nothing restarts when you change one."
            wide: true
            Layout.fillWidth: true

            TuningSection {
                host: page
                Layout.fillWidth: true
            }
        }
    }

    SettingsTab {
        title: "Swap files"
        iconName: "drive-harddisk"

        SettingsCard {
            title: "Swap on disk"
            iconName: "drive-harddisk"
            help: "Swap files are kept in /etc/fstab, so they come back after a restart."
            wide: true
            Layout.fillWidth: true

            SwapFilesSection {
                host: page
                Layout.fillWidth: true
            }
        }
    }

    SettingsTab {
        title: "Snapshots"
        iconName: "document-open-recent"

        SettingsCard {
            title: "Snapshots"
            iconName: "edit-undo"
            help: "XZram stores a copy of its settings before each change, so a change can be undone."
            wide: true
            Layout.fillWidth: true

            SnapshotsSection {
                host: page
                Layout.fillWidth: true
            }
        }
    }
}
