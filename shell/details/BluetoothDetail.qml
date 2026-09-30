import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Bluetooth
import qs
import qs.components
import qs.services

ColumnLayout {
    id: root

    readonly property bool usable: BluetoothState.available && BluetoothState.enabled

    spacing: Theme.spacing.md

    DetailHeader {
        Layout.fillWidth: true
        title: "Bluetooth"
        showSwitch: BluetoothState.available
        checked: BluetoothState.enabled
        switchEnabled: !BluetoothState.blocked
        onToggled: BluetoothState.toggle()
    }

    EmptyState {
        Layout.fillWidth: true
        Layout.topMargin: Theme.spacing.xl
        Layout.bottomMargin: Theme.spacing.xl
        visible: !root.usable
        icon: "bluetooth_disabled"
        title: !BluetoothState.available ? I18n.tr("No Bluetooth adapter") : BluetoothState.blocked ? I18n.tr("Bluetooth is blocked") : I18n.tr("Bluetooth is off")
        subtitle: !BluetoothState.available ? "" : BluetoothState.blocked ? I18n.tr("Check airplane mode (rfkill).") : I18n.tr("Turn it on to connect devices.")
    }

    ScrollArea {
        Layout.fillWidth: true
        Layout.fillHeight: true
        visible: root.usable
        spacing: Theme.spacing.xs

        SectionHeader {
            visible: BluetoothState.paired.length > 0
            text: I18n.tr("Paired devices")
            bottomPadding: Theme.spacing.xs
        }

        Repeater {
            model: ScriptModel {
                values: BluetoothState.paired
            }

            ListRow {
                id: pairedRow

                required property BluetoothDevice modelData

                width: parent.width
                icon: BluetoothState.deviceIcon(modelData)
                title: modelData.name
                subtitle: BluetoothState.stateText(modelData)
                highlighted: modelData.connected
                onClicked: modelData.connected ? modelData.disconnect() : modelData.connect()

                IconButton {
                    anchors.verticalCenter: parent.verticalCenter
                    implicitWidth: Theme.size.pillButton
                    implicitHeight: Theme.size.pillButton
                    tonal: false
                    icon: "delete"
                    iconSize: Theme.icon.small
                    iconColor: Theme.colors.textMuted
                    onClicked: pairedRow.modelData.forget()
                }
            }
        }

        SectionHeader {
            text: BluetoothState.discovered.length > 0 ? I18n.tr("Available devices") : I18n.tr("Searching for devices…")
            topPadding: BluetoothState.paired.length > 0 ? Theme.spacing.md : 0
            bottomPadding: Theme.spacing.xs
        }

        Repeater {
            model: ScriptModel {
                values: BluetoothState.discovered
            }

            ListRow {
                required property BluetoothDevice modelData

                width: parent.width
                icon: BluetoothState.deviceIcon(modelData)
                title: modelData.name
                subtitle: BluetoothState.pairingDevice === modelData ? I18n.tr("Pairing…") : BluetoothState.stateText(modelData)
                onClicked: BluetoothState.pair(modelData)
            }
        }
    }
}
