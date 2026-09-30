import QtQuick
import QtQuick.Layouts
import qs
import qs.components
import qs.services

// Battery + uptime on the left, settings/lock/power on the right. The power button
// expands a session menu with an inline confirmation step.
ColumnLayout {
    id: root

    property bool menuOpen: false
    // "" or one of "logout", "reboot", "poweroff" awaiting confirmation.
    property string pending: ""

    readonly property var actions: ({
            "logout": {
                label: I18n.tr("Log out"),
                icon: "logout",
                question: I18n.tr("Log out now?"),
                run: () => Session.logout()
            },
            "reboot": {
                label: I18n.tr("Restart"),
                icon: "restart_alt",
                question: I18n.tr("Restart now?"),
                run: () => Session.reboot()
            },
            "poweroff": {
                label: I18n.tr("Shut down"),
                icon: "power_settings_new",
                question: I18n.tr("Shut down now?"),
                run: () => Session.poweroff()
            }
        })

    spacing: Theme.spacing.sm

    Connections {
        target: ShellState

        function onPanelOpenChanged(): void {
            root.menuOpen = false;
            root.pending = "";
        }
    }

    RowLayout {
        Layout.fillWidth: true
        spacing: Theme.spacing.sm

        Rectangle {
            visible: Battery.available
            implicitHeight: Theme.size.pillButton
            implicitWidth: batteryRow.implicitWidth + 2 * Theme.spacing.md
            radius: height / 2
            color: Theme.colors.surfaceHigh

            RowLayout {
                id: batteryRow

                anchors.centerIn: parent
                spacing: Theme.spacing.xs

                MaterialIcon {
                    icon: Battery.icon
                    size: Theme.icon.small
                    fill: 1
                    color: Battery.percent <= 15 && !Battery.pluggedIn ? Theme.colors.error : Theme.colors.text
                }

                StyledText {
                    text: Battery.percent + "%"
                    font.weight: Theme.font.weightMedium
                }
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            spacing: 0

            StyledText {
                Layout.fillWidth: true
                visible: Battery.available
                text: Battery.stateText
                font.pixelSize: Theme.font.small
            }

            StyledText {
                Layout.fillWidth: true
                text: Session.uptime
                color: Theme.colors.textMuted
                font.pixelSize: Theme.font.small
            }
        }

        IconButton {
            icon: "settings"
            onClicked: Session.openSettings()
        }

        IconButton {
            icon: "lock"
            onClicked: Session.lock()
        }

        IconButton {
            icon: "power_settings_new"
            active: root.menuOpen
            onClicked: {
                root.menuOpen = !root.menuOpen;
                root.pending = "";
            }
        }
    }

    // Session menu
    Card {
        Layout.fillWidth: true
        Layout.preferredHeight: root.menuOpen ? Theme.size.pillButton + 2 * Theme.spacing.sm : 0
        clip: true
        opacity: root.menuOpen ? 1 : 0
        visible: Layout.preferredHeight > 0

        Behavior on Layout.preferredHeight {
            Anim {}
        }

        Behavior on opacity {
            Anim {}
        }

        RowLayout {
            anchors.fill: parent
            anchors.margins: Theme.spacing.sm
            spacing: Theme.spacing.sm
            visible: root.pending === ""

            Repeater {
                model: ["logout", "reboot", "poweroff"]

                PillButton {
                    required property string modelData

                    Layout.fillWidth: true
                    text: root.actions[modelData].label
                    icon: root.actions[modelData].icon
                    onClicked: root.pending = modelData
                }
            }
        }

        RowLayout {
            anchors.fill: parent
            anchors.margins: Theme.spacing.sm
            anchors.leftMargin: Theme.spacing.lg
            spacing: Theme.spacing.sm
            visible: root.pending !== ""

            StyledText {
                Layout.fillWidth: true
                text: root.pending !== "" ? root.actions[root.pending].question : ""
                font.weight: Theme.font.weightMedium
            }

            PillButton {
                text: I18n.tr("Cancel")
                style: "text"
                onClicked: root.pending = ""
            }

            PillButton {
                text: I18n.tr("Confirm")
                style: "danger"
                onClicked: root.actions[root.pending].run()
            }
        }
    }
}
