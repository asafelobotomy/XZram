// Asks before a change that moves swap in use. The question is a fixed text from logic.js
// (CONFIRMS), shown through UqLabel; the title and buttons are fixed strings.
import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.uq.settings

QQC2.Dialog {
    id: dialog

    /// The question to ask.
    property string question
    /// Emitted only when the person presses "Change".
    signal confirmed
    /// Emitted when the dialog is dismissed any other way.
    signal dismissed

    title: "Are you sure?"
    modal: true
    parent: QQC2.Overlay.overlay
    anchors.centerIn: parent
    width: Math.min(420, parent ? parent.width - 32 : 420)

    contentItem: UqLabel {
        text: dialog.question
        wrapMode: Text.Wrap
    }

    footer: QQC2.DialogButtonBox {
        QQC2.Button {
            objectName: "confirmChange"
            text: "Change"
            QQC2.DialogButtonBox.buttonRole: QQC2.DialogButtonBox.AcceptRole
        }
        QQC2.Button {
            objectName: "confirmCancel"
            text: "Cancel"
            QQC2.DialogButtonBox.buttonRole: QQC2.DialogButtonBox.RejectRole
        }
    }

    onAccepted: dialog.confirmed()
    onRejected: dialog.dismissed()
}
