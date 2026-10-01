import QtQuick
import Quickshell
import Quickshell.Services.Notifications
import Quickshell.Widgets
import qs
import qs.components
import qs.services

// Notification image, app icon or a generic bell, in a circle.
Item {
    id: root

    required property Notification notification
    property real size: Theme.size.notificationIcon

    readonly property bool claude: notification ? Notifications.isClaude(notification) : false
    readonly property string source: {
        const n = notification;
        if (!n)
            return "";
        if (root.claude && n.image === "")
            return Qt.resolvedUrl("../assets/claude.svg");
        if (n.image !== "")
            return n.image;
        const icon = n.appIcon;
        if (icon.startsWith("/"))
            return "file://" + icon;
        if (icon.startsWith("file://") || icon.startsWith("image://"))
            return icon;
        if (icon !== "")
            return Quickshell.iconPath(icon, true);
        const entry = n.desktopEntry !== "" ? DesktopEntries.byId(n.desktopEntry) : null;
        return entry?.icon ? Quickshell.iconPath(entry.icon, true) : "";
    }
    readonly property bool critical: notification?.urgency === NotificationUrgency.Critical

    implicitWidth: size
    implicitHeight: size

    ClippingRectangle {
        anchors.fill: parent
        radius: width / 2
        color: root.critical ? Theme.colors.errorSurface : Theme.colors.surfaceHighest

        IconImage {
            id: image

            anchors.fill: parent
            anchors.margins: root.notification?.image !== "" ? 0 : root.size * Theme.size.notificationIconInset
            visible: root.source !== "" && status === Image.Ready
            source: root.source
            asynchronous: true
        }

        MaterialIcon {
            anchors.centerIn: parent
            visible: !image.visible
            icon: root.critical ? "priority_high" : "notifications"
            size: root.size * Theme.size.notificationGlyphScale
            fill: 1
            color: root.critical ? Theme.colors.error : Theme.colors.textMuted
        }
    }
}
