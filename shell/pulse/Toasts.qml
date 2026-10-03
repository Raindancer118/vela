import QtQuick
import QtQuick.Layouts
import qs
import qs.components

// Results of actions: pop up at the bottom, stack, leave after a while.
Column {
    id: root

    spacing: Theme.spacing.sm

    move: Transition {
        SpringAnim {
            property: "y"
        }
        // An interrupted add/remove must not leave the row half visible.
        Anim {
            properties: "opacity,scale"
            to: 1
        }
        Anim {
            property: "x"
            to: 0
        }
    }

    Repeater {
        model: Pulse.toasts

        Rectangle {
            id: toast

            required property var modelData
            property bool shown: false

            anchors.horizontalCenter: parent.horizontalCenter
            implicitWidth: Math.min(560, row.implicitWidth + 2 * Theme.spacing.lg)
            implicitHeight: 44
            radius: 22
            color: Theme.colors.surfaceHighest
            border.width: 1
            border.color: modelData.ok ? Theme.colors.outline : Theme.withAlpha(Theme.pulse.crit, 0.5)
            opacity: shown ? 1 : 0
            scale: shown ? 1 : 0.9
            Behavior on opacity {
                Anim {}
            }
            Behavior on scale {
                SpringAnim {}
            }

            Rectangle {
                anchors.fill: parent
                radius: parent.radius
                color: Theme.colors.background
                z: -1
                opacity: 0.85
            }

            Component.onCompleted: shown = true

            Timer {
                running: true
                interval: toast.modelData.ok ? 3200 : 7000
                onTriggered: {
                    toast.shown = false;
                    gone.start();
                }
            }

            Timer {
                id: gone

                interval: Theme.anim.normal
                onTriggered: Pulse.dismissToast(toast.modelData.id)
            }

            RowLayout {
                id: row

                anchors.centerIn: parent
                spacing: Theme.spacing.sm

                MaterialIcon {
                    icon: toast.modelData.ok ? "check" : "warning"
                    color: toast.modelData.ok ? Theme.pulse.ok : Theme.pulse.crit
                    scale: toast.shown ? 1 : 0.3
                    Behavior on scale {
                        SpringAnim {
                            duration: Theme.anim.slow * 1.5
                        }
                    }
                }

                StyledText {
                    Layout.maximumWidth: 480
                    text: Words.result(toast.modelData)
                }
            }

            MouseArea {
                anchors.fill: parent
                onClicked: Pulse.dismissToast(toast.modelData.id)
            }
        }
    }
}
