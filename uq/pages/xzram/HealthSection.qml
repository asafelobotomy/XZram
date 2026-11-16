// The health check (`xzram doctor`): what it found, problems first, each with what to do.
import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.uq.settings
import "logic.js" as Logic

ColumnLayout {
    id: section

    property var host
    readonly property var doctor: section.host ? section.host.doctor : null

    spacing: 6

    SettingRow {
        settingId: "health"
        label: section.doctor ? section.doctor.summary : (section.host && section.host.doctorLoading ? "Checking…" : "Health check")
        highlight: section.host ? section.host.highlight : ""
        Layout.fillWidth: true

        QQC2.Button {
            objectName: "doctorButton"
            text: "Check again"
            enabled: !!section.host && !section.host.doctorLoading
            onClicked: section.host.readDoctor()
        }
    }
    QQC2.ProgressBar {
        visible: !!section.host && section.host.doctorLoading
        indeterminate: true
        Layout.fillWidth: true
    }
    ErrorCard {
        title: "Couldn't run the health check"
        detail: section.host ? section.host.doctorProblem : ""
        visible: detail !== ""
        Layout.fillWidth: true
    }
    Repeater {
        model: section.doctor ? section.doctor.issues : []

        ColumnLayout {
            required property var modelData
            spacing: 0
            Layout.fillWidth: true

            UqLabel {
                text: (modelData.severity === "error" ? "Problem: " : (modelData.severity === "warning" ? "Worth a look: " : "")) + modelData.message
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }
            UqLabel {
                text: modelData.suggestion
                visible: text !== ""
                wrapMode: Text.Wrap
                opacity: 0.7
                Layout.fillWidth: true
            }
        }
    }
    UqLabel {
        objectName: "allClear"
        visible: !!section.doctor && section.doctor.issues.length === 0
        text: "Nothing to fix."
        opacity: 0.8
    }
}
