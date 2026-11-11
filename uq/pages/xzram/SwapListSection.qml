// Swap areas, read-only: one plain line each. Names come from the system and are untrusted,
// so they are shown only through UqLabel.
import QtQuick
import QtQuick.Layouts
import org.uq.settings
import "logic.js" as Logic

SettingRow {
    id: row

    /// Rows from Logic.parseSwaps().
    property var swaps: []

    label: "Active swap"
    settingId: "swaps"

    ColumnLayout {
        spacing: 4

        UqLabel {
            visible: row.swaps.length === 0
            text: "No swap is set up."
        }
        Repeater {
            model: row.swaps
            UqLabel {
                required property var modelData
                text: Logic.swapLine(modelData)
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }
        }
    }
}
