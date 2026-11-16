// What a change said, under its row: a progress bar and what it is doing while it runs, then
// XZram's answer in plain words. A change refused for lack of memory offers to be saved for the
// next restart. What failed sits behind Advanced details.
import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.uq.settings
import "logic.js" as Logic

ColumnLayout {
    id: out

    /// The page (Page.qml): it holds the notes, the running change and its progress.
    property var host
    /// The row's key in the page's notes.
    property string noteKey
    readonly property var note: out.host ? out.host.noteFor(out.noteKey) : null
    readonly property bool working: !!out.host && out.host.workingKey === out.noteKey
    readonly property var progress: out.host ? out.host.progress : null
    readonly property var offer: out.host && out.host.offer && out.host.offer.key === out.noteKey ? out.host.offer : null
    property bool expanded: false

    visible: out.working || out.note !== null
    spacing: 4
    onNoteChanged: out.expanded = false

    QQC2.ProgressBar {
        objectName: "progressBar"
        visible: out.working
        indeterminate: !out.progress || out.progress.percent < 0
        value: out.progress && out.progress.percent >= 0 ? out.progress.percent / 100 : 0
        Layout.fillWidth: true
    }
    UqLabel {
        objectName: "outcomeMessage"
        id: message
        text: out.working ? (out.progress ? out.progress.label : Logic.WORKING) : (out.note ? out.note.message : "")
        visible: text !== ""
        wrapMode: Text.Wrap
        Layout.fillWidth: true
    }
    Binding {
        target: message
        property: "color"
        value: out.host ? out.host.errorColor() : "red"
        when: !out.working && out.note !== null && out.note.failed
    }
    UqLabel {
        objectName: "progressDetail"
        text: out.progress ? out.progress.detail : ""
        visible: out.working && text !== ""
        opacity: 0.7
        Layout.fillWidth: true
    }
    QQC2.Button {
        objectName: "offerButton"
        visible: !out.working && out.offer !== null
        text: "Save for the next restart"
        onClicked: out.host.takeOffer()
    }
    QQC2.Button {
        text: "Advanced details"
        flat: true
        visible: !out.working && out.note !== null && out.note.detail !== ""
        onClicked: out.expanded = !out.expanded
    }
    UqLabel {
        text: out.note ? out.note.detail : ""
        visible: out.expanded && !out.working && text !== ""
        wrapMode: Text.WrapAnywhere
        opacity: 0.8
        Layout.fillWidth: true
    }
}
