// Memory and swap: what XZram shows and the few things it changes. XZram's daemon is reached
// over the page's own D-Bus table (page.toml); reads happen when the page opens, after each
// change and on Refresh. All text goes through UqLabel; parsing and checks live in logic.js.
import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.uq.settings
import "logic.js" as Logic

Item {
    id: page

    property var ctx
    /// The [[setting]] id Settings search jumped to.
    property string highlight

    // What was last read (null until it has been).
    property var status: null
    property var zramConfig: null
    property int swappiness: -1
    property var swaps: []
    property bool zramKnown: false

    /// "loading", "unavailable", "error" or "ready".
    property string phase: "loading"
    property string problem
    property string problemText
    property string rawDetail
    property bool showRaw: false
    property string notice

    property bool busyZram: false
    property bool busySwappiness: false
    property bool moreAvailable: true
    property var pendingChanges: null
    /// The zram settings refused for lack of free memory, offered for the next restart.
    property string deferredJson
    /// Shared with delayed callbacks: set when the page is destroyed, so they do nothing.
    readonly property var life: ({ gone: false })
    Component.onDestruction: life.gone = true
    property int readSerial: 0

    readonly property var sizeRows: Logic.sizeChoices(page.zramConfig ? page.zramConfig.zram_size : null)
    readonly property var algorithmNames: Logic.algorithmChoices(page.zramConfig ? page.zramConfig.compression_algorithm : null)
    readonly property int sizeIndex: Logic.sizeIndex(page.zramConfig ? page.zramConfig.zram_size : null)
    readonly property int algorithmIndex: Logic.algorithmIndex(page.zramConfig ? page.zramConfig.compression_algorithm : null)
    readonly property bool zramEditable: page.zramConfig !== null && !page.busyZram
    readonly property color errorColor: (page.ctx && page.ctx.theme && page.ctx.theme.colors && page.ctx.theme.colors.error) || "red"

    // -----------------------------------------------------------------------------------------
    // Reading

    onCtxChanged: page.refresh()

    function refresh() {
        if (!page.ctx)
            return;
        const serial = ++page.readSerial;
        const life = page.life;
        const failures = [];
        let waiting = 4;
        function done(parse, take) {
            return Logic.guarded(life, function (reply) {
                if (serial !== page.readSerial)
                    return;
                const result = parse(reply);
                if (result.ok)
                    take(result);
                else
                    failures.push(result.error);
                if (--waiting === 0)
                    page.finishRead(failures);
            });
        }
        page.read("GetStatus", done(Logic.parseStatus, r => page.status = r));
        page.read("GetZramConfig", done(Logic.parseZramConfig, r => {
            page.zramConfig = r.config;
            page.zramKnown = true;
        }));
        page.read("GetSysctl", done(Logic.parseSysctl, r => page.swappiness = r.swappiness === null ? -1 : r.swappiness));
        page.read("ListSwaps", done(Logic.parseSwaps, r => page.swaps = r.swaps));
    }

    /// One read, tried once more when the service was still starting.
    function read(method, callback, attempt) {
        const life = page.life;
        page.ctx.dbus.call(method, [], Logic.guarded(life, reply => {
            if (reply && reply.error && !attempt && Logic.isRetryable(reply.error))
                page.read(method, callback, 1);
            else
                callback(reply);
        }));
    }

    function finishRead(failures) {
        if (failures.length === 0) {
            // Keep the raw text of a refused change; drop that of a failed read.
            if (page.phase !== "ready")
                page.rawDetail = "";
            page.phase = "ready";
            page.resetControls();
            return;
        }
        const e = Logic.describeError(failures[0], "read the memory settings");
        page.rawDetail = e.raw;
        page.phase = (e.kind === "unavailable" && page.status === null) ? "unavailable" : "error";
        page.problemText = e.kind === "other" ? "Couldn't read the memory settings." : e.message;
        page.resetControls();
    }

    // -----------------------------------------------------------------------------------------
    // Changing

    function resetControls() {
        sizeBox.currentIndex = Qt.binding(() => page.sizeIndex);
        algorithmBox.currentIndex = Qt.binding(() => page.algorithmIndex);
        spin.value = Qt.binding(() => Math.max(0, page.swappiness));
    }

    function askZram(changes) {
        // Check the value now, so a bad one never reaches the dialog.
        const built = Logic.buildZramConfig(page.zramConfig, changes);
        page.notice = "";
        if (!built.ok) {
            page.problem = built.error;
            page.resetControls();
            return;
        }
        page.problem = "";
        page.deferredJson = "";
        page.pendingChanges = changes;
        confirm.question = Logic.CONFIRMS[built.kind];
        confirm.open();
    }

    /// After Confirm: read the zram settings again, so the other fields sent are current,
    /// then write.
    function runZram() {
        const changes = page.pendingChanges;
        page.pendingChanges = null;
        if (!changes || !page.ctx)
            return;
        const life = page.life;
        page.busyZram = true;
        page.ctx.dbus.call("GetZramConfig", [], Logic.guarded(life, fresh => {
            const current = Logic.parseZramConfig(fresh);
            const built = current.ok ? Logic.buildZramConfig(current.config, changes) : { ok: false, error: "Couldn't read the zram settings, so nothing was changed." };
            if (!built.ok) {
                page.busyZram = false;
                page.problem = built.error;
                page.resetControls();
                return;
            }
            page.ctx.dbus.call("ApplyNowZram", [built.json], Logic.guarded(life, reply => {
                page.busyZram = false;
                const lowMemory = reply && reply.error && Logic.describeError(reply.error, "").kind === "memory";
                page.deferredJson = lowMemory ? built.json : "";
                page.finishWrite(reply, "change the zram settings",
                    built.kind === "zram-size" ? "Size changed." : "Compression changed.");
            }));
        }));
    }

    /// After a refusal for lack of memory: save the same settings for the next restart.
    function runZramAtBoot() {
        const json = page.deferredJson;
        page.deferredJson = "";
        if (json === "" || !page.ctx)
            return;
        page.busyZram = true;
        page.ctx.dbus.call("ApplyZramWithOptions", [json, true, false], Logic.guarded(page.life, reply => {
            page.busyZram = false;
            page.finishWrite(reply, "save the zram settings", Logic.AT_BOOT_DONE);
        }));
    }

    function cancelZram() {
        // Cancel is not a failure: put the controls back and say nothing.
        page.pendingChanges = null;
        page.resetControls();
    }

    function applySwappiness() {
        const built = Logic.buildSysctl(spin.value);
        page.notice = "";
        if (!built.ok) {
            page.problem = built.error;
            page.resetControls();
            return;
        }
        page.problem = "";
        page.deferredJson = "";
        page.busySwappiness = true;
        const life = page.life;
        const chosen = spin.value;
        page.ctx.dbus.call("ApplyNowSysctl", [built.json], Logic.guarded(life, reply => {
            page.busySwappiness = false;
            page.finishWrite(reply, "change swappiness", "Swappiness set to " + chosen + ".");
        }));
    }

    function finishWrite(reply, what, done) {
        const failed = reply && reply.error ? Logic.describeError(reply.error, what) : null;
        page.rawDetail = "";
        if (failed === null) {
            page.problem = "";
            page.notice = done;
        } else if (failed.kind === "pending") {
            // The work may still be going: not a failure.
            page.problem = "";
            page.notice = failed.message;
        } else {
            page.notice = "";
            page.problem = failed.message;
            page.rawDetail = failed.raw;
        }
        page.resetControls();
        page.refresh();
    }

    function openXzram() {
        if (!page.ctx)
            return;
        const life = page.life;
        page.ctx.openApp("io.github.XZram.desktop", Logic.guarded(life, reply => {
            if (reply && reply.error) {
                page.moreAvailable = false;
                page.notice = "Couldn't open XZram.";
            }
        }));
    }

    function scrollTo(id) {
        const rows = { "zram-size": sizeRow, "zram-algorithm": algorithmRow, "swappiness": swappinessRow, "swaps": swapsRow };
        const target = rows[id];
        if (target)
            flick.contentY = Math.max(0, Math.min(target.y, flick.contentHeight - flick.height));
    }

    onHighlightChanged: page.scrollTo(page.highlight)

    ConfirmDialog {
        id: confirm
        objectName: "confirmDialog"
        onConfirmed: page.runZram()
        onDismissed: page.cancelZram()
    }

    Flickable {
        id: flick
        anchors.fill: parent
        contentWidth: width
        contentHeight: column.implicitHeight + 32
        clip: true
        QQC2.ScrollBar.vertical: QQC2.ScrollBar {}

        ColumnLayout {
            id: column
            x: 16
            y: 16
            width: flick.width - 32
            spacing: 12

            UqLabel {
                objectName: "phaseLabel"
                visible: page.phase === "loading"
                text: "Checking…"
                Layout.fillWidth: true
            }
            UqLabel {
                objectName: "unavailableLabel"
                visible: page.phase === "unavailable"
                text: page.problemText
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }
            ErrorCard {
                objectName: "readError"
                visible: page.phase === "error"
                title: "Couldn't read the memory settings"
                detail: page.problemText
                Layout.fillWidth: true
            }
            QQC2.Button {
                objectName: "advancedButton"
                visible: (page.phase === "error" || page.phase === "unavailable" || page.problem !== "") && page.rawDetail !== ""
                text: "Advanced details"
                onClicked: page.showRaw = !page.showRaw
            }
            UqLabel {
                objectName: "rawDetail"
                visible: page.showRaw && page.rawDetail !== ""
                text: page.rawDetail
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }

            UqLabel {
                objectName: "problemLabel"
                visible: page.problem !== ""
                text: page.problem
                color: page.errorColor
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }
            QQC2.Button {
                objectName: "atBootButton"
                visible: page.deferredJson !== "" && !page.busyZram
                text: "Apply at next restart"
                onClicked: page.runZramAtBoot()
            }
            UqLabel {
                objectName: "noticeLabel"
                visible: page.notice !== ""
                text: page.notice
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }

            StatusSection {
                lines: Logic.statusLines(page.status)
                loading: page.phase === "loading"
                Layout.fillWidth: true
            }

            UqLabel {
                objectName: "notSetUp"
                visible: page.phase === "ready" && page.zramKnown && page.zramConfig === null
                text: "Compressed memory isn't set up through XZram on this computer, so its size and compression can't be changed here."
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }

            SettingRow {
                id: sizeRow
                settingId: "zram-size"
                label: "Compressed memory size"
                highlight: page.highlight
                Layout.fillWidth: true

                UqComboBox {
                    id: sizeBox
                    objectName: "sizeBox"
                    model: page.sizeRows.map(r => r.label)
                    currentIndex: page.sizeIndex
                    enabled: page.zramEditable
                    onActivated: index => {
                        const row = page.sizeRows[index];
                        if (!row || row.value === "" || index === page.sizeIndex)
                            page.resetControls();
                        else
                            page.askZram({ size: row.value });
                    }
                }
                UqLabel {
                    visible: page.busyZram
                    text: "Working…"
                }
            }

            SettingRow {
                id: algorithmRow
                settingId: "zram-algorithm"
                label: "Compression"
                highlight: page.highlight
                Layout.fillWidth: true

                UqComboBox {
                    id: algorithmBox
                    objectName: "algorithmBox"
                    model: page.algorithmNames
                    currentIndex: page.algorithmIndex
                    enabled: page.zramEditable
                    onActivated: index => {
                        const name = page.algorithmNames[index];
                        if (name === undefined || name === "Default" || index === page.algorithmIndex)
                            page.resetControls();
                        else
                            page.askZram({ algorithm: name });
                    }
                }
                UqLabel {
                    visible: page.busyZram
                    text: "Working…"
                }
            }

            SettingRow {
                id: swappinessRow
                settingId: "swappiness"
                label: "Swappiness"
                highlight: page.highlight
                Layout.fillWidth: true

                QQC2.SpinBox {
                    id: spin
                    objectName: "swappinessSpin"
                    from: 0
                    to: 200
                    editable: true
                    value: Math.max(0, page.swappiness)
                    enabled: page.swappiness >= 0 && !page.busySwappiness
                }
                QQC2.Button {
                    objectName: "applySwappiness"
                    text: "Apply"
                    enabled: page.swappiness >= 0 && spin.value !== page.swappiness && !page.busySwappiness
                    onClicked: page.applySwappiness()
                }
                UqLabel {
                    visible: page.busySwappiness
                    text: "Working…"
                }
            }
            UqLabel {
                text: "Higher values move memory to swap sooner. 0 to 200; the usual choice is 60, or higher with compressed memory."
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }

            SwapListSection {
                id: swapsRow
                swaps: page.swaps
                highlight: page.highlight
                Layout.fillWidth: true
            }

            RowLayout {
                spacing: 8

                QQC2.Button {
                    objectName: "refreshButton"
                    text: "Refresh"
                    onClicked: page.refresh()
                }
                QQC2.Button {
                    objectName: "moreButton"
                    visible: page.moreAvailable
                    text: "More in XZram"
                    onClicked: page.openXzram()
                }
            }
        }
    }
}
