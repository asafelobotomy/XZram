// Each compressed-memory (zram) device: what it holds, what that takes, and how well it squeezes.
import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.uq.settings
import "logic.js" as Logic

ColumnLayout {
    id: section

    property var host
    readonly property var devices: section.host && section.host.status ? section.host.status.zram : []

    spacing: 8

    UqLabel {
        objectName: "noZram"
        visible: section.devices.length === 0
        text: "There is no compressed memory right now."
        wrapMode: Text.Wrap
        opacity: 0.8
        Layout.fillWidth: true
    }
    Repeater {
        model: section.devices

        ColumnLayout {
            required property var modelData
            spacing: 2
            Layout.fillWidth: true

            UqLabel {
                text: modelData.name + ": " + (modelData.algorithm || "unknown") + ", " + Logic.size(modelData.sizeBytes) + (modelData.active ? "" : " (not in use)")
                font.bold: true
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }
            QQC2.ProgressBar {
                value: Logic.fraction(modelData.storedBytes, modelData.sizeBytes)
                Layout.fillWidth: true
            }
            UqLabel {
                text: modelData.storedBytes > 0
                    ? Logic.size(modelData.storedBytes) + " stored in " + Logic.size(modelData.compressedBytes) + (modelData.ratio > 0 ? " (" + modelData.ratio + " to 1)" : "")
                    : "Nothing stored yet."
                wrapMode: Text.Wrap
                opacity: 0.8
                Layout.fillWidth: true
            }
        }
    }
}
