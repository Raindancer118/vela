import QtQuick
import qs

// Pill-shaped single line input. With `password` the text is masked and an
// eye button reveals it.
Rectangle {
    id: root

    property alias text: input.text
    property alias input: input
    property string placeholder
    property bool password: false
    property bool reveal: false

    signal accepted

    implicitHeight: Theme.size.iconButton + Theme.spacing.xs
    implicitWidth: Theme.size.controlWidth
    radius: height / 2
    color: Theme.colors.surfaceHighest
    border.width: Theme.size.border
    border.color: input.activeFocus ? Theme.colors.primary : "transparent"

    Behavior on border.color {
        ColorAnim {}
    }

    TextInput {
        id: input

        anchors.left: parent.left
        anchors.right: eye.visible ? eye.left : parent.right
        anchors.leftMargin: Theme.spacing.lg
        anchors.rightMargin: Theme.spacing.sm
        anchors.verticalCenter: parent.verticalCenter
        clip: true
        color: Theme.colors.text
        selectionColor: Theme.colors.primaryMuted
        selectedTextColor: Theme.colors.text
        font.family: Theme.font.family
        font.pixelSize: Theme.font.body
        echoMode: root.password && !root.reveal ? TextInput.Password : TextInput.Normal
        onAccepted: root.accepted()

        StyledText {
            anchors.fill: parent
            visible: input.text === ""
            text: root.placeholder
            color: Theme.colors.textMuted
        }
    }

    IconButton {
        id: eye

        visible: root.password
        anchors.right: parent.right
        anchors.rightMargin: Theme.spacing.xs
        anchors.verticalCenter: parent.verticalCenter
        implicitWidth: Theme.size.pillButton
        implicitHeight: Theme.size.pillButton
        tonal: false
        icon: root.reveal ? "visibility_off" : "visibility"
        iconSize: Theme.icon.small
        onClicked: root.reveal = !root.reveal
    }
}
