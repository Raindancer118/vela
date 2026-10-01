import QtQuick
import qs

// Round icon button like the launcher's gear and the settings' header
// buttons: flat, a tile surface on hover; `active` is the accent selection,
// `tonal` a resting tile surface.
Rectangle {
    id: root

    property string icon
    property bool active: false
    property bool tonal: false
    property real iconSize: Theme.icon.normal
    property color iconColor: active ? Theme.colors.primary : Theme.colors.text
    readonly property alias hovered: area.containsMouse

    signal clicked

    implicitWidth: Theme.size.iconButton
    implicitHeight: Theme.size.iconButton
    radius: height / 2
    opacity: enabled ? 1 : Theme.opacity.disabled
    color: active ? Theme.colors.selected : tonal ? Theme.colors.tile : "transparent"
    border.width: active ? Theme.size.border : 0
    border.color: Theme.colors.selectedRing

    Behavior on color {
        ColorAnim {}
    }

    Clickable {
        id: area

        radius: root.radius
        onClicked: root.clicked()
    }

    MaterialIcon {
        anchors.centerIn: parent
        icon: root.icon
        size: root.iconSize
        fill: root.active ? 1 : 0
        color: root.iconColor

        Behavior on color {
            ColorAnim {}
        }
    }
}
