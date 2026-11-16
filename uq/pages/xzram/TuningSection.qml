// Kernel memory settings: one number each, with the range, what it does and the usual choice.
// Each Apply shows what it will do and asks first.
import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.uq.settings
import "logic.js" as Logic

ColumnLayout {
    id: section

    property var host
    readonly property var tuning: section.host && section.host.config ? section.host.config.tuning : null

    spacing: 10

    UqLabel {
        objectName: "tuningUnknown"
        visible: section.tuning === null
        text: "Looking…"
        opacity: 0.7
    }
    Repeater {
        model: section.tuning === null ? [] : Logic.TUNING

        ColumnLayout {
            id: item
            required property var modelData
            readonly property var current: section.tuning[item.modelData.key]
            spacing: 2
            Layout.fillWidth: true

            SettingRow {
                settingId: item.modelData.key === "swappiness" ? "swappiness" : "tuning"
                label: item.modelData.label
                highlight: section.host ? section.host.highlight : ""
                Layout.fillWidth: true

                NumberBox {
                    id: spin
                    objectName: "spin-" + item.modelData.key
                    from: item.modelData.min
                    to: item.modelData.max
                    value: item.current === null ? item.modelData.usual : item.current
                    enabled: item.current !== null && !!section.host && !section.host.busy
                }
                QQC2.Button {
                    objectName: "apply-" + item.modelData.key
                    text: "Apply"
                    enabled: item.current !== null && spin.typed !== item.current && !!section.host && !section.host.busy
                    onClicked: section.host.changeTuning(item.modelData.key, String(spin.typed), () => {
                        spin.value = Qt.binding(() => item.current === null ? item.modelData.usual : item.current);
                    })
                }
            }
            UqLabel {
                text: item.modelData.help + " " + item.modelData.min + " to " + item.modelData.max + "; usual choice " + item.modelData.usual + "."
                wrapMode: Text.Wrap
                opacity: 0.7
                Layout.fillWidth: true
            }
            Outcome {
                host: section.host
                noteKey: item.modelData.key
                Layout.fillWidth: true
            }
        }
    }
}
