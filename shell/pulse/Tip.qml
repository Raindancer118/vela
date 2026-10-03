import QtQuick
import qs
import qs.components

// Small tooltip above its parent while `shown` (usually the parent's hover),
// after a short delay.
Item {
    id: root

    property string text
    property bool shown: false
    property bool ready: false

    anchors.fill: parent
    z: 100
    onShownChanged: {
        if (shown)
            delay.restart();
        else {
            delay.stop();
            ready = false;
        }
    }

    Timer {
        id: delay

        interval: 450
        onTriggered: root.ready = true
    }

    Rectangle {
        anchors.bottom: parent.top
        anchors.bottomMargin: 6
        anchors.horizontalCenter: parent.horizontalCenter
        width: label.implicitWidth + 2 * Theme.spacing.sm
        height: label.implicitHeight + Theme.spacing.xs * 2
        radius: Theme.radius.small
        color: Theme.colors.background
        border.width: 1
        border.color: Theme.colors.outline
        opacity: root.ready && root.text !== "" ? 1 : 0
        visible: opacity > 0
        scale: root.ready ? 1 : 0.92
        Behavior on opacity {
            Anim {
                duration: Theme.anim.fast
            }
        }
        Behavior on scale {
            SpringAnim {
                duration: Theme.anim.normal
            }
        }

        StyledText {
            id: label

            anchors.centerIn: parent
            text: root.text
            font.pixelSize: Theme.font.small
        }
    }
}
