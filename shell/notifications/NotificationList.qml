import QtQuick
import QtQuick.Layouts
import Quickshell
import qs
import qs.components
import qs.services

ColumnLayout {
    id: root

    // "Clear all" lets the groups slide out one after another and only
    // dismisses the notifications once the last one is gone. Snapshot, so
    // notifications arriving meanwhile stay.
    property var clearing: []
    readonly property var clearingApps: clearing.map(n => Notifications.appKey(n))

    function clearAll(): void {
        if (clearing.length > 0 || Notifications.count === 0)
            return;
        clearing = Notifications.list;
        const steps = Math.min(Notifications.apps.length, Theme.anim.staggerMaxSteps) - 1;
        clearTimer.interval = Theme.anim.normal + steps * Theme.anim.stagger;
        clearTimer.restart();
    }

    function leaveDelay(index: int): int {
        return Math.min(index, Theme.anim.staggerMaxSteps - 1) * Theme.anim.stagger;
    }

    spacing: Theme.spacing.sm

    Timer {
        id: clearTimer

        onTriggered: {
            Notifications.dismissMany(root.clearing);
            clearDone.restart();
        }
    }

    // The removed groups stay "leaving" (invisible) until the ListView's
    // remove transition has destroyed them, or they'd flash up again.
    Timer {
        id: clearDone

        interval: Theme.anim.normal
        onTriggered: root.clearing = []
    }

    RowLayout {
        Layout.fillWidth: true
        spacing: Theme.spacing.sm

        SectionHeader {
            text: I18n.tr("Notifications")
        }

        Rectangle {
            opacity: Notifications.count > 0 && root.clearing.length === 0 ? 1 : 0
            visible: opacity > 0
            implicitHeight: countText.implicitHeight + 4
            implicitWidth: Math.max(implicitHeight, countText.implicitWidth + 2 * Theme.spacing.sm)
            radius: height / 2
            // Like the launcher's result badges.
            color: Theme.colors.tile

            Behavior on opacity {
                Anim {}
            }

            Behavior on implicitWidth {
                SpringAnim {
                    duration: Theme.anim.normal
                }
            }

            StyledText {
                id: countText

                anchors.centerIn: parent
                text: Notifications.count
                color: Theme.colors.textMuted
                font.pixelSize: Math.round(Theme.font.small * 0.92)
                font.weight: Theme.font.weightSemiBold
            }
        }

        Item {
            Layout.fillWidth: true
        }

        PillButton {
            opacity: Notifications.count > 0 && root.clearing.length === 0 ? 1 : 0
            visible: opacity > 0
            enabled: opacity === 1
            style: "text"
            icon: "clear_all"
            text: I18n.tr("Clear all")
            onClicked: root.clearAll()

            Behavior on opacity {
                Anim {}
            }
        }
    }

    // Do-not-disturb hint above the cards; unfolds and folds away (the empty
    // state has its own hint).
    Item {
        id: dndHint

        readonly property bool shown: Notifications.dnd && Notifications.count > root.clearing.length

        Layout.fillWidth: true
        Layout.preferredHeight: shown ? hintRow.implicitHeight : 0
        Layout.topMargin: shown ? 0 : -parent.spacing
        visible: Layout.preferredHeight > 0
        opacity: shown ? 1 : 0
        clip: true

        Behavior on Layout.preferredHeight {
            Anim {}
        }

        Behavior on Layout.topMargin {
            Anim {}
        }

        Behavior on opacity {
            Anim {}
        }

        Rectangle {
            id: hintRow

            width: parent.width
            implicitHeight: hintLayout.implicitHeight + 2 * Theme.spacing.sm
            radius: Theme.radius.small
            color: Theme.colors.surface

            RowLayout {
                id: hintLayout

                anchors.fill: parent
                anchors.leftMargin: Theme.spacing.md
                anchors.rightMargin: Theme.spacing.md
                spacing: Theme.spacing.sm

                MaterialIcon {
                    icon: "do_not_disturb_on"
                    size: Theme.icon.small
                    fill: 1
                    color: Theme.colors.textMuted
                    scale: dndHint.shown ? 1 : Theme.anim.swapScale

                    Behavior on scale {
                        SpringAnim {}
                    }
                }

                StyledText {
                    Layout.fillWidth: true
                    text: I18n.tr("Do not disturb is on. New notifications won't pop up.")
                    color: Theme.colors.textMuted
                    font.pixelSize: Theme.font.small
                    wrapMode: Text.Wrap
                }
            }
        }
    }

    Item {
        Layout.fillWidth: true
        Layout.fillHeight: true

        ListView {
            id: view

            anchors.fill: parent
            clip: true
            spacing: Theme.spacing.sm
            boundsBehavior: Flickable.StopAtBounds
            model: ScriptModel {
                values: Notifications.apps
            }

            Connections {
                target: ShellState

                function onPanelOpenChanged(): void {
                    if (ShellState.panelOpen)
                        view.forceLayout();
                }
            }

            delegate: NotificationGroup {
                required property string modelData
                required property int index

                width: view.width
                app: modelData
                leaving: root.clearingApps.includes(modelData)
                leaveDelay: root.leaveDelay(index)
            }

            add: Transition {
                ParallelAnimation {
                    Anim {
                        property: "opacity"
                        from: 0
                        to: 1
                    }

                    Anim {
                        property: "x"
                        from: Theme.anim.slideShort
                        to: 0
                        easing.bezierCurve: Theme.anim.emphasizedDecel
                    }
                }
            }

            remove: Transition {
                ParallelAnimation {
                    Anim {
                        property: "opacity"
                        to: 0
                    }

                    Anim {
                        property: "x"
                        to: Theme.anim.slideDistance
                    }
                }
            }
        }

        EmptyState {
            id: empty

            anchors.centerIn: parent
            width: parent.width
            visible: opacity > 0
            opacity: 0
            scale: Theme.anim.popInScale
            states: State {
                name: "shown"
                // During "Clear all" already while the last card leaves.
                when: Notifications.count === root.clearing.length

                PropertyChanges {
                    empty.opacity: 1
                    empty.scale: 1
                }
            }
            transitions: [
                Transition {
                    to: "shown"

                    // Waits for the last card to be gone.
                    SequentialAnimation {
                        PauseAnimation {
                            duration: root.clearing.length > 0 ? clearTimer.interval : Theme.anim.fast
                        }

                        ParallelAnimation {
                            Anim {
                                property: "opacity"
                            }

                            SpringAnim {
                                property: "scale"
                            }
                        }
                    }
                },
                Transition {
                    from: "shown"

                    Anim {
                        properties: "opacity,scale"
                        duration: Theme.anim.fast
                    }
                }
            ]
            icon: Notifications.dnd ? "do_not_disturb_on" : "notifications_active"
            title: I18n.tr("All caught up")
            subtitle: Notifications.dnd ? I18n.tr("Do not disturb is on. New notifications will still show up here.") : I18n.tr("No notifications")
        }
    }
}
