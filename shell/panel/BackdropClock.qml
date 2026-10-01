import QtQuick
import Quickshell
import qs
import qs.components
import qs.services

// Clock and date large in the middle of the blurred area left of the panel
// (vela settings → Panel → Clock on the blur). Steps aside for detail cards.
Column {
    id: root

    // 0..1, the control center's open animation.
    property real progress: 1

    visible: Config.clockOnBackdrop && opacity > 0
    opacity: root.progress * (ShellState.detail === "" ? 1 : 0)
    spacing: Theme.spacing.sm

    Behavior on opacity {
        enabled: root.progress === 1

        Anim {
            duration: Theme.anim.normal
        }
    }

    SystemClock {
        id: clock

        enabled: Config.clockOnBackdrop && ShellState.panelOpen
        precision: SystemClock.Minutes
    }

    StyledText {
        anchors.horizontalCenter: parent.horizontalCenter
        text: Config.locale.toString(clock.date, Config.timeFormat)
        font.pixelSize: Theme.font.backdropClock
        font.family: Theme.font.clockFamily
        font.weight: Theme.font.weightLight
    }

    StyledText {
        anchors.horizontalCenter: parent.horizontalCenter
        text: Config.locale.toString(clock.date, Config.dateFormat)
        color: Theme.colors.textMuted
        font.family: Theme.font.clockFamily
        font.pixelSize: Theme.font.backdropDate
    }
}
