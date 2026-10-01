import QtQuick
import QtQuick.Layouts
import Quickshell
import qs
import qs.components
import qs.services

// All notifications of one app in a card. Shows the newest few; the rest
// behind "Show more".
Card {
    id: root

    required property string app
    // Slides out after `leaveDelay` ms ("Clear all"); the list removes it
    // afterwards.
    property bool leaving: false
    property int leaveDelay: 0

    readonly property var items: Notifications.list.filter(n => Notifications.appKey(n) === app)
    readonly property int collapsedCount: Config.groupCollapsedCount
    property bool expanded: false
    // Compact: one mini row (icon, app, count) until clicked open.
    property bool open: false
    readonly property bool mini: Config.compactNotifications && !open
    readonly property var shown: expanded ? items : items.slice(0, collapsedCount)

    // Grows and shrinks smoothly when notifications come, go or "Show more"
    // is toggled.
    implicitHeight: column.implicitHeight
    clip: true

    // Only while visible: animating in the hidden panel left the ListView
    // with stale positions (cards overlapping after it opened).
    Behavior on implicitHeight {
        enabled: ShellState.panelOpen

        Anim {}
    }

    transform: Translate {
        id: shift
    }

    // Moves smoothly when the list repositions it (insertions, removals,
    // cards above growing). Unlike a ListView displaced transition, whose
    // target is fixed when it starts, this retargets on every change, so a
    // card above that grows mid-animation can't leave this one overlapping.
    property bool placed: false

    Component.onCompleted: Qt.callLater(() => root.placed = true)

    Behavior on y {
        enabled: root.placed && ShellState.panelOpen

        SpringAnim {
            duration: Theme.anim.normal
        }
    }

    // Opacity/x of the delegate itself belong to the ListView transitions.
    SequentialAnimation {
        running: root.leaving
        onStopped: if (!root.leaving) {
            root.opacity = 1;
            shift.x = 0;
        }

        PauseAnimation {
            duration: root.leaveDelay
        }

        ParallelAnimation {
            Anim {
                target: root
                property: "opacity"
                to: 0
            }

            Anim {
                target: shift
                property: "x"
                to: Theme.anim.slideDistance
                easing.bezierCurve: Theme.anim.emphasizedDecel
            }
        }
    }

    ColumnLayout {
        id: column

        width: parent.width
        spacing: 0

        // Mini row: small icon, app name, count; click opens the group.
        Item {
            Layout.fillWidth: true
            visible: root.mini
            implicitHeight: miniRow.implicitHeight + 2 * Theme.spacing.sm

            HoverHandler {
                id: miniHover
            }

            Clickable {
                radius: Theme.radius.card
                onClicked: root.open = true
            }

            RowLayout {
                id: miniRow

                anchors.left: parent.left
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                anchors.leftMargin: Theme.spacing.md
                anchors.rightMargin: Theme.spacing.sm
                spacing: Theme.spacing.sm

                NotificationIcon {
                    notification: root.items[0] ?? null
                    size: Theme.size.notificationIconMini
                }

                StyledText {
                    Layout.fillWidth: true
                    text: root.items.length > 1 ? root.app + "  ·  " + root.items.length : root.app
                    color: Theme.colors.textMuted
                    font.pixelSize: Theme.font.small
                    font.weight: Theme.font.weightMedium
                }

                IconButton {
                    opacity: miniHover.hovered ? 1 : 0
                    implicitWidth: Theme.icon.normal
                    implicitHeight: Theme.icon.normal
                    tonal: false
                    icon: "close"
                    iconSize: Theme.icon.small * 0.8
                    iconColor: Theme.colors.textMuted
                    onClicked: Notifications.dismissApp(root.app)

                    Behavior on opacity {
                        Anim {
                            duration: Theme.anim.fast
                        }
                    }
                }
            }
        }

        RowLayout {
            Layout.fillWidth: true
            Layout.leftMargin: Theme.spacing.lg
            Layout.rightMargin: Theme.spacing.sm
            Layout.topMargin: Theme.spacing.sm
            visible: !root.mini
            spacing: Theme.spacing.sm

            StyledText {
                Layout.fillWidth: true
                text: root.items.length > 1 ? root.app + "  ·  " + root.items.length : root.app
                color: Theme.colors.textMuted
                font.pixelSize: Theme.font.small
                font.weight: Theme.font.weightMedium
            }

            PillButton {
                visible: root.items.length > root.collapsedCount
                implicitHeight: Theme.icon.large
                style: "text"
                text: root.expanded ? I18n.tr("Show less") : I18n.tr("Show %1 more", root.items.length - root.collapsedCount)
                onClicked: root.expanded = !root.expanded
            }

            // Compact mode: fold the group back into its mini row.
            IconButton {
                visible: Config.compactNotifications
                implicitWidth: Theme.icon.large
                implicitHeight: Theme.icon.large
                tonal: false
                icon: "unfold_less"
                iconSize: Theme.icon.small
                iconColor: Theme.colors.textMuted
                onClicked: root.open = false
            }

            IconButton {
                visible: root.items.length > 1
                implicitWidth: Theme.icon.large
                implicitHeight: Theme.icon.large
                tonal: false
                icon: "clear_all"
                iconSize: Theme.icon.small
                iconColor: Theme.colors.textMuted
                onClicked: Notifications.dismissApp(root.app)
            }
        }

        Repeater {
            model: ScriptModel {
                values: root.shown
            }

            NotificationContent {
                id: item

                required property var modelData

                Layout.fillWidth: true
                visible: !root.mini
                notification: modelData
                allowCompact: false
                showAppName: false
                onClicked: item.expanded = !item.expanded
            }
        }
    }
}
