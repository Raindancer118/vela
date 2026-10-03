import QtQuick
import Quickshell
import qs
import qs.components

// App icon from the icon theme; a coloured initial when there is none.
Item {
    id: root

    property string icon
    property string name
    property string fallback: ""
    property real size: 24

    // Claude (Claude Code, the desktop app) always with its own logo.
    readonly property bool isClaude: icon.toLowerCase() === "claude" || name.toLowerCase() === "claude" || icon.toLowerCase().startsWith("claude-")
    readonly property string source: {
        if (isClaude)
            return Qt.resolvedUrl("../assets/claude.svg");
        const a = icon !== "" ? Quickshell.iconPath(icon, true) : "";
        if (a !== "")
            return a;
        return fallback !== "" ? Quickshell.iconPath(fallback, true) : "";
    }

    implicitWidth: size
    implicitHeight: size

    Image {
        anchors.fill: parent
        visible: root.source !== ""
        source: root.source
        sourceSize: Qt.size(root.size * 2, root.size * 2)
        smooth: true
        asynchronous: true
    }

    Rectangle {
        visible: root.source === ""
        anchors.fill: parent
        radius: width * 0.3
        color: {
            let h = 0;
            for (const c of root.name)
                h = (h * 31 + c.charCodeAt(0)) % 360;
            return Qt.hsla(h / 360, 0.45, Theme.pulse.light ? 0.55 : 0.42, 1);
        }

        StyledText {
            anchors.centerIn: parent
            text: (root.name || "?").charAt(0).toUpperCase()
            color: "white"
            font.pixelSize: root.size * 0.5
            font.weight: Theme.font.weightSemiBold
        }
    }
}
