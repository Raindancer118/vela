import QtQuick
import QtQuick.Layouts
import Quickshell.Services.Notifications
import qs
import qs.components
import qs.services

// One notification: icon, summary, time, body and actions. Swipe it
// sideways or press the close button to dismiss it.
Item {
    id: root

    required property Notification notification
    property bool showAppName: true
    property bool expanded: false

    signal clicked

    readonly property bool critical: notification?.urgency === NotificationUrgency.Critical
    readonly property var buttons: notification ? notification.actions.filter(a => a.identifier !== "default" && a.text !== "") : []

    // Slides out sideways, collapses so the rows below move up smoothly,
    // then dismisses (swipe and close button).
    function dismissAnimated(direction: int): void {
        if (dismissAnim.running)
            return;
        content.x = direction * content.width;
        dismissAnim.start();
    }

    property real collapse: 1

    implicitHeight: (layout.implicitHeight + 2 * Theme.spacing.md) * collapse
    clip: true

    // Swipe to dismiss.
    Item {
        id: content

        width: parent.width
        height: parent.height
        opacity: (1 - Math.min(1, Math.abs(x) / width)) * appear.value

        // Fades in when it appears inside an existing group or "Show more".
        QtObject {
            id: appear

            property real value: 0

            Component.onCompleted: fadeIn.start()
        }

        Anim {
            id: fadeIn

            target: appear
            property: "value"
            to: 1
        }

        Behavior on x {
            enabled: !drag.active

            Anim {}
        }

        DragHandler {
            id: drag

            target: content
            xAxis.enabled: true
            yAxis.enabled: false
            onActiveChanged: {
                if (active)
                    return;
                if (Math.abs(content.x) > Theme.size.dragDismissThreshold) {
                    root.dismissAnimated(content.x > 0 ? 1 : -1);
                } else {
                    content.x = 0;
                }
            }
        }

        SequentialAnimation {
            id: dismissAnim

            // Runs alongside the x Behavior above.
            PauseAnimation {
                duration: Theme.anim.normal
            }

            Anim {
                target: root
                property: "collapse"
                to: 0
            }

            ScriptAction {
                script: Notifications.dismiss(root.notification)
            }
        }

        Clickable {
            radius: Theme.radius.small
            onClicked: root.clicked()
        }

        RowLayout {
            id: layout

            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.margins: Theme.spacing.md
            spacing: Theme.spacing.md

            NotificationIcon {
                Layout.alignment: Qt.AlignTop
                notification: root.notification
            }

            ColumnLayout {
                Layout.fillWidth: true
                spacing: Theme.spacing.xs

                RowLayout {
                    Layout.fillWidth: true
                    spacing: Theme.spacing.xs

                    StyledText {
                        Layout.fillWidth: true
                        text: root.notification?.summary || root.notification?.appName || ""
                        color: root.critical ? Theme.colors.error : Theme.colors.text
                        font.weight: Theme.font.weightMedium
                    }

                    StyledText {
                        text: {
                            const time = root.notification ? Notifications.relativeTime(root.notification) : "";
                            return root.showAppName && root.notification?.appName ? root.notification.appName + " · " + time : time;
                        }
                        color: Theme.colors.textMuted
                        font.pixelSize: Theme.font.small
                        Layout.maximumWidth: layout.width * Theme.size.notificationMetaMaxShare
                    }

                    IconButton {
                        Layout.alignment: Qt.AlignTop
                        implicitWidth: Theme.icon.large
                        implicitHeight: Theme.icon.large
                        tonal: false
                        icon: "close"
                        iconSize: Theme.icon.small
                        iconColor: Theme.colors.textMuted
                        onClicked: root.dismissAnimated(1)
                    }
                }

                StyledText {
                    Layout.fillWidth: true
                    visible: text !== ""
                    // No images: StyledText would load remote <img> sources.
                    text: (root.notification?.body ?? "").replace(/<img[^>]*>/gi, "").replace(/\n/g, "<br/>")
                    textFormat: Text.StyledText
                    wrapMode: Text.Wrap
                    maximumLineCount: root.expanded ? Theme.size.notificationBodyLinesExpanded : Theme.size.notificationBodyLines
                    color: Theme.colors.textMuted
                    linkColor: Theme.colors.primary
                    onLinkActivated: link => Qt.openUrlExternally(link)
                }

                Flow {
                    Layout.fillWidth: true
                    Layout.topMargin: Theme.spacing.xs
                    visible: root.buttons.length > 0
                    spacing: Theme.spacing.sm

                    Repeater {
                        model: root.buttons

                        PillButton {
                            required property NotificationAction modelData

                            text: modelData.text
                            onClicked: modelData.invoke()
                        }
                    }
                }
            }
        }
    }
}
