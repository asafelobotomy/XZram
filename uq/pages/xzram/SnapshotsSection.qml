// Snapshots of the memory and swap settings: XZram takes one before each change, so a change
// can be undone. Restoring, deleting and trimming show what they will do and ask first.
import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.uq.settings
import "logic.js" as Logic

ColumnLayout {
    id: section

    property var host
    readonly property var rows: section.host && section.host.snapshots ? section.host.snapshots.snapshots : []
    readonly property bool hasBefore: section.rows.some(s => s.before)

    spacing: 8

    SettingRow {
        settingId: "snapshots"
        label: section.rows.length === 0 ? "No snapshots yet" : Logic.plural(section.rows.length, "snapshot", "snapshots")
        highlight: section.host ? section.host.highlight : ""
        Layout.fillWidth: true

        QQC2.Button {
            objectName: "rollbackButton"
            text: "Undo the last change"
            enabled: section.hasBefore && !!section.host && !section.host.busy
            onClicked: section.host.rollBack()
        }
        QQC2.Button {
            objectName: "snapshotNow"
            text: "Take a snapshot now"
            enabled: !!section.host && !section.host.busy
            onClicked: section.host.takeSnapshot()
        }
    }
    Outcome {
        host: section.host
        noteKey: "snapshot-new"
        Layout.fillWidth: true
    }
    Outcome {
        host: section.host
        noteKey: "rollback"
        Layout.fillWidth: true
    }
    ErrorCard {
        title: "Couldn't read the snapshots"
        detail: section.host ? section.host.snapshotsProblem : ""
        visible: detail !== ""
        Layout.fillWidth: true
    }
    Repeater {
        model: section.rows

        ColumnLayout {
            id: row
            required property var modelData
            spacing: 2
            Layout.fillWidth: true

            SettingRow {
                label: row.modelData.label
                Layout.fillWidth: true

                UqLabel {
                    text: row.modelData.when + ", " + row.modelData.trigger
                    opacity: 0.8
                }
                QQC2.Button {
                    objectName: "restore-" + row.modelData.id
                    text: "Restore"
                    enabled: !!section.host && !section.host.busy
                    onClicked: section.host.restoreSnapshot(row.modelData.id)
                }
                QQC2.Button {
                    objectName: "delete-" + row.modelData.id
                    text: "Delete"
                    enabled: !!section.host && !section.host.busy
                    onClicked: section.host.deleteSnapshot(row.modelData.id)
                }
            }
            Outcome {
                host: section.host
                noteKey: "snapshot:" + row.modelData.id
                Layout.fillWidth: true
            }
        }
    }
    RowLayout {
        visible: section.rows.length > 1
        spacing: 8
        Layout.topMargin: 8
        Layout.fillWidth: true

        UqLabel {
            text: "Keep only the newest"
        }
        NumberBox {
            id: keepSpin
            objectName: "keepSpin"
            from: 1
            to: 100000
            value: 50
        }
        QQC2.Button {
            objectName: "pruneButton"
            text: "Delete older ones"
            enabled: !!section.host && !section.host.busy
            onClicked: section.host.pruneSnapshots(keepSpin.typed)
        }
    }
    Outcome {
        host: section.host
        noteKey: "snapshot-prune"
        Layout.fillWidth: true
    }
}
