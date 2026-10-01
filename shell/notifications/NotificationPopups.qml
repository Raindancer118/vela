import QtQuick
import Quickshell
import Quickshell.Services.Notifications
import Quickshell.Wayland
import qs
import qs.components
import qs.services

// Popups in the top right corner while the panel is closed. Only the cards
// take input; everything around them is click-through.
PanelWindow {
    id: root

    property ShellScreen popupScreen: null
    // Stays true until the last remove animation has finished.
    property bool shown: false

    visible: shown
    screen: popupScreen
    color: "transparent"
    anchors {
        top: true
        right: true
        bottom: true
    }
    margins {
        top: Theme.size.screenMargin
        right: Theme.size.screenMargin
    }
    implicitWidth: Theme.size.notificationWidth
    exclusionMode: ExclusionMode.Ignore
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: "quickshell-notifications"
    WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
    mask: Region {
        item: view
    }

    Connections {
        target: Notifications

        function onPopupsChanged(): void {
            if (Notifications.popups.length === 0) {
                hideDelay.restart();
                return;
            }
            // Pick the monitor when the first popup appears; keep it while
            // popups are showing so they don't jump between screens.
            if (!root.shown)
                root.popupScreen = ShellState.focusedScreen();
            hideDelay.stop();
            root.shown = true;
        }
    }

    Timer {
        id: hideDelay

        interval: Theme.anim.normal
        onTriggered: root.shown = false
    }

    ListView {
        id: view

        width: parent.width
        height: contentHeight
        interactive: false
        spacing: Theme.spacing.sm
        model: ScriptModel {
            values: Notifications.popups
        }

        delegate: Card {
            id: card

            required property Notification modelData

            width: view.width
            implicitHeight: content.implicitHeight
            color: Theme.colors.panel
            border.width: Theme.size.border
            border.color: Theme.colors.outline

            HoverHandler {
                id: hover
            }


            Timer {
                // expireTimeout is the raw D-Bus value in milliseconds.
                interval: card.modelData?.expireTimeout > 0 ? Math.min(card.modelData.expireTimeout, Config.popupMaxTimeout) : Config.popupTimeout
                running: !hover.hovered && !(Config.criticalPopupsStay && card.modelData?.urgency === NotificationUrgency.Critical)
                onTriggered: Notifications.hidePopup(card.modelData)
            }

            NotificationContent {
                id: content

                anchors.fill: parent
                notification: card.modelData
                // Popups are meant to be read: never compact.
                allowCompact: false
                onClicked: Notifications.activate(card.modelData)
            }
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
                    from: Theme.anim.slideDistance
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

        // Also restores opacity/x: an add transition interrupted by a
        // displacement would otherwise leave the item half transparent.
        displaced: Transition {
            ParallelAnimation {
                Anim {
                    property: "y"
                }

                Anim {
                    property: "opacity"
                    to: 1
                }

                Anim {
                    property: "x"
                    to: 0
                }
            }
        }
    }
}
