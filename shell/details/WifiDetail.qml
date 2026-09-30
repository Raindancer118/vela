import QtQuick
import QtQuick.Layouts
import Quickshell
import qs
import qs.components
import qs.services

ColumnLayout {
    id: root

    readonly property bool usable: NetworkState.usable
    readonly property var others: NetworkState.networks.filter(n => !n.connected)

    spacing: Theme.spacing.md

    DetailHeader {
        Layout.fillWidth: true
        title: I18n.tr("Wi-Fi")
        showSwitch: NetworkState.wifiAvailable
        checked: NetworkState.wifiEnabled
        switchEnabled: !NetworkState.wifiBlockedByHardware
        onToggled: NetworkState.toggleWifi()

        IconButton {
            visible: root.usable
            icon: "refresh"
            tonal: false
            onClicked: NetworkState.rescan()
        }
    }

    EmptyState {
        Layout.fillWidth: true
        Layout.topMargin: Theme.spacing.xl
        Layout.bottomMargin: Theme.spacing.xl
        visible: !root.usable
        icon: "signal_wifi_off"
        title: !NetworkState.wifiAvailable ? I18n.tr("No Wi-Fi adapter") : NetworkState.wifiBlockedByHardware ? I18n.tr("Wi-Fi is blocked") : I18n.tr("Wi-Fi is off")
        subtitle: !NetworkState.wifiAvailable ? "" : NetworkState.wifiBlockedByHardware ? I18n.tr("Check the hardware switch or airplane mode.") : I18n.tr("Turn it on to see networks.")
    }

    Card {
        Layout.fillWidth: true
        visible: root.usable && NetworkState.activeNetwork !== null
        implicitHeight: Theme.size.listRow + Theme.spacing.sm

        ListRow {
            anchors.fill: parent
            anchors.margins: Theme.spacing.xs
            clickable: false
            iconFill: 1
            icon: NetworkState.signalIcon(NetworkState.activeNetwork?.signalStrength ?? 0)
            title: NetworkState.activeNetwork?.name ?? ""
            subtitle: NetworkState.activeNetwork ? I18n.tr("Connected · %1", NetworkState.securityName(NetworkState.activeNetwork)) : ""

            PillButton {
                anchors.verticalCenter: parent.verticalCenter
                text: I18n.tr("Disconnect")
                onClicked: NetworkState.activeNetwork?.disconnect()
            }
        }
    }

    SectionHeader {
        visible: root.usable
        text: root.others.length > 0 ? I18n.tr("Available networks") : I18n.tr("Searching for networks…")
    }

    ScrollArea {
        Layout.fillWidth: true
        Layout.fillHeight: true
        visible: root.usable

        Repeater {
            model: ScriptModel {
                values: root.others
            }

            WifiNetworkRow {
                required property var modelData

                width: parent.width
                network: modelData
            }
        }
    }
}
