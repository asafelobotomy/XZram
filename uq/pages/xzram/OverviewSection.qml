// Memory and swap in use, each compressed-memory device, and the health check. Read-only:
// the numbers come from `xzram status`, which the page re-reads every few seconds while this
// tab is open.
import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.uq.settings
import "logic.js" as Logic

ColumnLayout {
    id: section

    property var host
    readonly property var status: section.host ? section.host.status : null
    readonly property var doctor: section.host ? section.host.doctor : null

    spacing: 8

    // One line of text over a bar.
    component Meter: ColumnLayout {
        property string caption
        property real filled: 0
        spacing: 2
        Layout.fillWidth: true

        UqLabel {
            text: parent.caption
            wrapMode: Text.Wrap
            Layout.fillWidth: true
        }
        QQC2.ProgressBar {
            value: parent.filled
            Layout.fillWidth: true
        }
    }

    SettingRow {
        settingId: "memory"
        label: "Memory and swap in use"
        highlight: section.host ? section.host.highlight : ""
        Layout.fillWidth: true

        QQC2.Button {
            objectName: "refreshButton"
            text: "Refresh"
            onClicked: section.host.refreshAll()
        }
    }
    ErrorCard {
        objectName: "statusError"
        title: "Couldn't read memory and swap"
        detail: section.host ? section.host.statusProblem : ""
        visible: detail !== ""
        Layout.fillWidth: true
    }
    UqLabel {
        text: "Looking…"
        visible: section.status === null && section.host && section.host.statusProblem === ""
        opacity: 0.7
    }
    Meter {
        objectName: "memoryMeter"
        visible: section.status !== null
        caption: section.status ? "Memory: " + Logic.size(section.status.memory.usedBytes) + " in use of " + Logic.size(section.status.memory.totalBytes) : ""
        filled: section.status ? Logic.fraction(section.status.memory.usedBytes, section.status.memory.totalBytes) : 0
    }
    Meter {
        objectName: "swapMeter"
        visible: section.status !== null && section.status.memory.swapTotalBytes > 0
        caption: section.status ? "Swap: " + Logic.size(section.status.memory.swapUsedBytes) + " in use of " + Logic.size(section.status.memory.swapTotalBytes) : ""
        filled: section.status ? Logic.fraction(section.status.memory.swapUsedBytes, section.status.memory.swapTotalBytes) : 0
    }
    UqLabel {
        objectName: "noSwap"
        visible: section.status !== null && section.status.memory.swapTotalBytes === 0
        text: "Swap: none in use. When memory fills up, apps are slowed or stopped."
        wrapMode: Text.Wrap
        Layout.fillWidth: true
    }
    UqLabel {
        objectName: "pressureLabel"
        visible: section.status !== null
        text: section.status ? "Memory pressure: " + Logic.pressureText(section.status.pressure) : ""
        wrapMode: Text.Wrap
        opacity: 0.8
        Layout.fillWidth: true
    }
}
