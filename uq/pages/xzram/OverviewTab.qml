// The Overview tab of the Memory and swap page: what is in use, the compressed memory devices,
// and XZram's health checks. `host` is the page (Page.qml).
import QtQuick
import QtQuick.Layouts
import org.uq.settings

SettingsTab {
    id: tab

    property var host

    title: "Overview"
    iconName: "utilities-system-monitor"

    SettingsCard {
        title: "Memory and swap"
        iconName: "media-flash"
        help: "What is in use right now. It updates every few seconds while this tab is open."
        wide: true
        Layout.fillWidth: true

        OverviewSection {
            host: tab.host
            Layout.fillWidth: true
        }
    }
    SettingsCard {
        title: "Compressed memory"
        iconName: "package-x-generic"
        help: "Swap that lives in memory and is compressed, which is faster than a disk."
        Layout.fillWidth: true

        ZramDevices {
            host: tab.host
            Layout.fillWidth: true
        }
    }
    SettingsCard {
        title: "Health"
        iconName: "dialog-information"
        help: "XZram looks for settings that would make the computer freeze when memory runs out."
        Layout.fillWidth: true

        HealthSection {
            host: tab.host
            Layout.fillWidth: true
        }
    }
}
