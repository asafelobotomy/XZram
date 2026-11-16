// Compressed memory: its size, how it compresses, and which swap is used first. Every change
// shows what it will do and asks first (uQ's confirmation sheet); when there isn't enough free
// memory to move swap back, the page says so and offers to save it for the next restart.
import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.uq.settings
import "logic.js" as Logic

ColumnLayout {
    id: section

    property var host
    readonly property var zram: section.host && section.host.config ? section.host.config.zram : null
    readonly property bool editable: section.zram !== null && section.zram.configured && !!section.host && !section.host.busy
    readonly property var sizeRows: Logic.sizeChoices(section.zram ? section.zram.size : null)
    readonly property var algorithmNames: Logic.algorithmChoices(section.zram ? section.zram.algorithm : null)

    spacing: 8

    function reset() {
        sizeBox.currentIndex = Qt.binding(() => Logic.sizeIndex(section.zram ? section.zram.size : null));
        algorithmBox.currentIndex = Qt.binding(() => Logic.algorithmIndex(section.zram ? section.zram.algorithm : null));
    }

    // Nothing set up yet.
    ColumnLayout {
        visible: section.zram !== null && !section.zram.configured
        spacing: 6
        Layout.fillWidth: true

        UqLabel {
            objectName: "notSetUp"
            text: "Compressed memory isn't set up through XZram on this computer. Setting it up gives swap that lives in memory and is compressed, which is faster than a disk."
            wrapMode: Text.Wrap
            Layout.fillWidth: true
        }
        QQC2.Button {
            objectName: "setUpButton"
            text: "Set up compressed memory"
            enabled: !!section.host && !section.host.busy
            onClicked: section.host.changeZram("zram-algorithm", "zstd")
        }
        Outcome {
            host: section.host
            noteKey: "zram-algorithm"
            Layout.fillWidth: true
        }
    }

    SettingRow {
        id: sizeRow
        settingId: "zram-size"
        label: "Compressed memory size"
        highlight: section.host ? section.host.highlight : ""
        visible: section.zram !== null && section.zram.configured
        Layout.fillWidth: true

        UqComboBox {
            id: sizeBox
            objectName: "sizeBox"
            model: section.sizeRows.map(r => r.label)
            currentIndex: Logic.sizeIndex(section.zram ? section.zram.size : null)
            enabled: section.editable
            onActivated: index => {
                const row = section.sizeRows[index];
                if (!row || row.value === "" || index === Logic.sizeIndex(section.zram ? section.zram.size : null))
                    section.reset();
                else
                    section.host.changeZram("zram-size", row.value, section.reset);
            }
        }
    }
    Outcome {
        host: section.host
        noteKey: "zram-size"
        visible: section.zram !== null && section.zram.configured && (working || note !== null)
        Layout.fillWidth: true
    }

    SettingRow {
        id: algorithmRow
        settingId: "zram-algorithm"
        label: "Compression"
        highlight: section.host ? section.host.highlight : ""
        visible: section.zram !== null && section.zram.configured
        Layout.fillWidth: true

        UqComboBox {
            id: algorithmBox
            objectName: "algorithmBox"
            model: section.algorithmNames
            currentIndex: Logic.algorithmIndex(section.zram ? section.zram.algorithm : null)
            enabled: section.editable
            onActivated: index => {
                const name = section.algorithmNames[index];
                if (name === undefined || name === "Default" || index === Logic.algorithmIndex(section.zram ? section.zram.algorithm : null))
                    section.reset();
                else
                    section.host.changeZram("zram-algorithm", name, section.reset);
            }
        }
    }
    UqLabel {
        visible: section.zram !== null && section.zram.configured && text !== ""
        text: section.zram && section.zram.algorithm ? (Logic.ALGORITHM_HELP[section.zram.algorithm] || "") : ""
        wrapMode: Text.Wrap
        opacity: 0.7
        Layout.fillWidth: true
    }
    Outcome {
        host: section.host
        noteKey: "zram-algorithm"
        visible: section.zram !== null && section.zram.configured && (working || note !== null)
        Layout.fillWidth: true
    }

    SettingRow {
        id: priorityRow
        settingId: "zram-priority"
        label: "Priority"
        highlight: section.host ? section.host.highlight : ""
        visible: section.zram !== null && section.zram.configured
        Layout.fillWidth: true

        NumberBox {
            id: prioritySpin
            objectName: "prioritySpin"
            from: Logic.PRIORITY_MIN
            to: Logic.PRIORITY_MAX
            value: section.zram && section.zram.priority !== null ? section.zram.priority : 100
            enabled: section.editable
        }
        QQC2.Button {
            objectName: "applyPriority"
            text: "Apply"
            enabled: section.editable && section.zram !== null && prioritySpin.typed !== section.zram.priority
            onClicked: section.host.changeZram("zram-priority", String(prioritySpin.typed), () => {
                prioritySpin.value = Qt.binding(() => section.zram && section.zram.priority !== null ? section.zram.priority : 100);
            })
        }
    }
    UqLabel {
        visible: section.zram !== null && section.zram.configured
        text: "Which swap is used first: higher goes first. Compressed memory is normally ahead of any disk swap."
        wrapMode: Text.Wrap
        opacity: 0.7
        Layout.fillWidth: true
    }
    Outcome {
        host: section.host
        noteKey: "zram-priority"
        visible: section.zram !== null && section.zram.configured && (working || note !== null)
        Layout.fillWidth: true
    }

    ColumnLayout {
        visible: section.zram !== null && section.zram.configured
        spacing: 4
        Layout.fillWidth: true

        QQC2.Button {
            objectName: "turnOffButton"
            text: "Turn off compressed memory"
            enabled: section.editable
            onClicked: section.host.turnZramOff()
        }
        Outcome {
            host: section.host
            noteKey: "zram-off"
            Layout.fillWidth: true
        }
    }
}
