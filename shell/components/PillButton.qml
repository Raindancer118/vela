import QtQuick
import QtQuick.Layouts
import qs

// Text button with an optional leading icon.
// style: "filled" (primary), "tonal" (dark surface), "text" (transparent),
// "danger" (error color).
Rectangle {
    id: root

    property string text
    property string icon
    property string style: "tonal"

    readonly property color contentColor: {
        switch (style) {
        case "filled":
            return Theme.colors.textOnPrimary;
        case "danger":
            return Theme.colors.textOnError;
        case "text":
            return Theme.colors.primary;
        default:
            return Theme.colors.text;
        }
    }

    signal clicked

    implicitHeight: Theme.size.pillButton
    implicitWidth: row.implicitWidth + 2 * Theme.spacing.lg
    radius: height / 2
    opacity: enabled ? 1 : Theme.opacity.disabled
    color: {
        switch (style) {
        case "filled":
            return Theme.colors.primary;
        case "danger":
            return Theme.colors.error;
        case "text":
            return "transparent";
        default:
            return Theme.colors.surfaceHighest;
        }
    }

    Behavior on color {
        ColorAnim {}
    }

    Clickable {
        radius: root.radius
        inverted: root.style === "filled" || root.style === "danger"
        onClicked: root.clicked()
    }

    RowLayout {
        id: row

        anchors.centerIn: parent
        spacing: Theme.spacing.xs

        MaterialIcon {
            visible: root.icon !== ""
            icon: root.icon
            size: Theme.icon.small
            color: root.contentColor
        }

        StyledText {
            text: root.text
            color: root.contentColor
            font.weight: Theme.font.weightMedium
        }
    }
}
