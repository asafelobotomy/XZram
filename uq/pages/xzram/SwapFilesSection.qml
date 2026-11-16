// Swap areas on disk: the swap files in /etc/fstab can be resized or removed, and a new one
// created. Every change shows what it will do and asks first; one that would bring more back
// into memory than is free is refused before anything is touched.
import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.uq.settings
import "logic.js" as Logic

ColumnLayout {
    id: section

    property var host
    readonly property var files: section.host && section.host.swapfiles ? section.host.swapfiles.files : []
    readonly property var areas: section.host && section.host.status ? section.host.status.swaps : []
    /// Swap that isn't zram and isn't a listed file (a partition, say): shown, not changed here.
    readonly property var others: section.areas.filter(a => a.name.indexOf("/dev/zram") !== 0 && !section.files.some(f => f.path === a.name))
    /// The path of the file being resized, "" when none.
    property string resizing: ""

    spacing: 8

    function used(path) {
        const hit = section.areas.find(a => a.name === path);
        return hit ? hit.usedBytes : -1;
    }

    ErrorCard {
        title: "Couldn't read the swap files"
        detail: section.host ? section.host.swapfilesProblem : ""
        visible: detail !== ""
        Layout.fillWidth: true
    }
    SettingRow {
        settingId: "swaps"
        label: section.files.length === 0 ? "No swap files" : Logic.plural(section.files.length, "swap file", "swap files")
        highlight: section.host ? section.host.highlight : ""
        Layout.fillWidth: true
    }
    UqLabel {
        objectName: "noFiles"
        visible: section.files.length === 0 && (!section.host || section.host.swapfilesProblem === "")
        text: "There are no swap files. Compressed memory is usually enough; a swap file on disk is a safety net for when it isn't."
        wrapMode: Text.Wrap
        opacity: 0.8
        Layout.fillWidth: true
    }
    Repeater {
        model: section.files

        ColumnLayout {
            id: row
            required property var modelData
            spacing: 4
            Layout.fillWidth: true

            SettingRow {
                label: row.modelData.path
                highlight: section.host ? section.host.highlight : ""
                Layout.fillWidth: true

                UqLabel {
                    text: Logic.size(row.modelData.sizeMb * 1048576) + (section.used(row.modelData.path) >= 0 ? ", " + Logic.size(section.used(row.modelData.path)) + " in use" : ", not in use")
                    opacity: 0.8
                }
                QQC2.Button {
                    objectName: "resize-" + row.modelData.path
                    text: "Resize"
                    enabled: !!section.host && !section.host.busy
                    onClicked: {
                        section.resizing = section.resizing === row.modelData.path ? "" : row.modelData.path;
                        resizeField.text = String(row.modelData.sizeMb / 1024);
                    }
                }
                QQC2.Button {
                    objectName: "remove-" + row.modelData.path
                    text: "Remove"
                    enabled: !!section.host && !section.host.busy
                    onClicked: section.host.removeSwapfile(row.modelData.path)
                }
            }
            RowLayout {
                visible: section.resizing === row.modelData.path
                spacing: 8
                Layout.fillWidth: true

                UqLabel {
                    text: "New size in GB:"
                }
                QQC2.TextField {
                    id: resizeField
                    objectName: "resizeField"
                    Layout.preferredWidth: 100
                    inputMethodHints: Qt.ImhFormattedNumbersOnly
                }
                QQC2.Button {
                    objectName: "resizeApply"
                    text: "Resize"
                    enabled: !!section.host && !section.host.busy
                    onClicked: {
                        if (section.host.resizeSwapfile(row.modelData.path, resizeField.text))
                            section.resizing = "";
                    }
                }
            }
            Outcome {
                host: section.host
                noteKey: "swapfile:" + row.modelData.path
                Layout.fillWidth: true
            }
        }
    }

    Repeater {
        model: section.others

        SettingRow {
            required property var modelData
            label: modelData.name
            Layout.fillWidth: true

            UqLabel {
                text: (modelData.type ? modelData.type + ", " : "") + Logic.size(modelData.usedBytes) + " in use of " + Logic.size(modelData.sizeBytes)
                opacity: 0.8
            }
        }
    }

    // A new swap file.
    UqLabel {
        text: "Add a swap file"
        font.bold: true
        Layout.topMargin: 8
    }
    RowLayout {
        spacing: 8
        Layout.fillWidth: true

        UqComboBox {
            id: pathBox
            objectName: "newPath"
            model: Logic.SWAPFILE_PATHS
            Layout.preferredWidth: 200
        }
        QQC2.TextField {
            id: newSize
            objectName: "newSize"
            placeholderText: "Size in GB"
            text: "4"
            Layout.preferredWidth: 100
            inputMethodHints: Qt.ImhFormattedNumbersOnly
        }
        QQC2.Button {
            objectName: "createButton"
            text: "Create"
            enabled: !!section.host && !section.host.busy
            onClicked: section.host.createSwapfile(Logic.SWAPFILE_PATHS[pathBox.currentIndex], newSize.text)
        }
    }
    Outcome {
        host: section.host
        noteKey: "swapfile-new"
        Layout.fillWidth: true
    }
}
