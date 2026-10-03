import QtQuick
import QtQuick.Layouts
import qs

// Text button with an optional leading icon, shaped like GTK's buttons.
// style: "filled" (accent selection), "tonal" (tile surface), "text"
// (flat, accent text), "danger" (error).
Rectangle {
    id: root

    property string text
    property string icon
    // A picture in the icon's place (in its own colours, e.g. a logo).
    property url image
    property string style: "tonal"

    readonly property color contentColor: {
        switch (style) {
        case "filled":
            return Theme.colors.text;
        case "danger":
            return Theme.colors.textOnError;
        case "text":
            return Theme.colors.primary;
        default:
            return Theme.colors.text;
        }
    }

    readonly property alias hovered: area.containsMouse
    signal clicked

    implicitHeight: Theme.size.pillButton
    implicitWidth: row.implicitWidth + 2 * Theme.spacing.lg
    radius: Theme.radius.small
    opacity: enabled ? 1 : Theme.opacity.disabled
    color: {
        switch (style) {
        case "filled":
            return Theme.colors.selected;
        case "danger":
            return Theme.colors.error;
        case "text":
            return "transparent";
        default:
            return Theme.colors.tile;
        }
    }
    border.width: style === "filled" ? Theme.size.border : 0
    border.color: Theme.colors.selectedRing

    Behavior on color {
        ColorAnim {}
    }

    Clickable {
        id: area

        radius: root.radius
        inverted: root.style === "danger"
        onClicked: root.clicked()
    }

    RowLayout {
        id: row

        anchors.centerIn: parent
        spacing: Theme.spacing.xs

        Image {
            visible: root.image.toString() !== ""
            source: root.image
            sourceSize: Qt.size(Theme.icon.small * 2, Theme.icon.small * 2)
            Layout.preferredWidth: Theme.icon.small
            Layout.preferredHeight: Theme.icon.small
        }

        MaterialIcon {
            visible: root.icon !== "" && root.image.toString() === ""
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
