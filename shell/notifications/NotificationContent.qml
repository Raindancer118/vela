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
    // Mini (vela settings → Compact): just a small icon and the app's name
    // until expanded. Groups in the panel handle compactness themselves.
    property bool allowCompact: true
    readonly property bool compact: allowCompact && Config.compactNotifications && !expanded
    readonly property string plainBody: (notification?.body ?? "").replace(/<[^>]*>/g, "").replace(/\s+/g, " ").trim()
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

    implicitHeight: (layout.implicitHeight + 2 * pad) * collapse
    readonly property real pad: compact ? Theme.spacing.sm : Theme.spacing.md
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
            anchors.margins: root.pad
            anchors.leftMargin: Theme.spacing.md
            spacing: root.compact ? Theme.spacing.sm : Theme.spacing.md

            NotificationIcon {
                Layout.alignment: root.compact ? Qt.AlignVCenter : Qt.AlignTop
                notification: root.notification
                size: root.compact ? Theme.size.notificationIconMini : Theme.size.notificationIcon
            }

            ColumnLayout {
                Layout.fillWidth: true
                spacing: Theme.spacing.xs

                RowLayout {
                    Layout.fillWidth: true
                    spacing: Theme.spacing.xs

                    StyledText {
                        Layout.fillWidth: !root.compact || inlineBody.text === ""
                        Layout.maximumWidth: root.compact ? layout.width * Theme.size.notificationCompactTitleShare : -1
                        text: root.compact ? (root.notification ? Notifications.appKey(root.notification) : "") : root.notification?.summary || root.notification?.appName || ""
                        color: root.critical ? Theme.colors.error : root.compact ? Theme.colors.textMuted : Theme.colors.text
                        font.pixelSize: root.compact ? Theme.font.small : Theme.font.body
                        font.weight: Theme.font.weightMedium
                    }

                    // Compact: the text follows the title on the same line.
                    StyledText {
                        id: inlineBody

                        Layout.fillWidth: true
                        visible: false
                        text: ""
                    }

                    StyledText {
                        visible: !root.compact
                        text: {
                            const time = root.notification ? Notifications.relativeTime(root.notification) : "";
                            // Compact rows keep the room for the text.
                            return root.showAppName && !root.compact && root.notification?.appName ? root.notification.appName + " · " + time : time;
                        }
                        color: Theme.colors.textMuted
                        font.pixelSize: Theme.font.small
                        Layout.maximumWidth: layout.width * Theme.size.notificationMetaMaxShare
                    }

                    IconButton {
                        visible: !root.compact
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
                    visible: !root.compact && text !== ""
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
                    visible: !root.compact && root.buttons.length > 0
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
