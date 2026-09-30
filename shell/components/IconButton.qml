import QtQuick
import qs

// Round icon button. `active` shows the highlighted (primary) style,
// `tonal` a dark filled background, otherwise it is transparent.
Rectangle {
    id: root

    property string icon
    property bool active: false
    property bool tonal: true
    property real iconSize: Theme.icon.normal
    property color iconColor: active ? Theme.colors.textOnPrimary : Theme.colors.text
    readonly property alias hovered: area.containsMouse

    signal clicked

    implicitWidth: Theme.size.iconButton
    implicitHeight: Theme.size.iconButton
    radius: height / 2
    opacity: enabled ? 1 : Theme.opacity.disabled
    color: active ? Theme.colors.primary : tonal ? Theme.colors.surfaceHigh : "transparent"

    Behavior on color {
        ColorAnim {}
    }

    Clickable {
        id: area

        radius: root.radius
        inverted: root.active
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
