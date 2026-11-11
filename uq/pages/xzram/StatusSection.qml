// The memory and swap summary: a few read-only lines. Every line is plain text from logic.js.
import QtQuick
import QtQuick.Layouts
import org.uq.settings

ColumnLayout {
    id: section

    /// Lines to show, from Logic.statusLines().
    property var lines: []
    property bool loading: false

    spacing: 4

    UqLabel {
        visible: section.loading && section.lines.length === 0
        text: "Checking…"
        Layout.fillWidth: true
    }
    Repeater {
        model: section.lines
        UqLabel {
            required property string modelData
            text: modelData
            wrapMode: Text.Wrap
            Layout.fillWidth: true
        }
    }
}
