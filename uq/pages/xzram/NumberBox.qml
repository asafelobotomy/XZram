// A whole-number box. `typed` is the number in the box right now, typed or not: a SpinBox only
// commits typing on Enter or when it loses focus (`live` needs Qt 6.6), and Apply must not wait.
import QtQuick
import QtQuick.Controls as QQC2

QQC2.SpinBox {
    id: box

    property int typed: box.value

    editable: true
    onValueChanged: box.typed = box.value

    Connections {
        target: box.contentItem
        ignoreUnknownSignals: true

        function onTextChanged() {
            const n = parseInt(box.contentItem.text, 10);
            box.typed = isNaN(n) || n < box.from || n > box.to ? box.value : n;
        }
    }
}
