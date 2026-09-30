import QtQuick
import QtQuick.Layouts
import Quickshell
import qs
import qs.components
import qs.services

ColumnLayout {
    spacing: 0

    SystemClock {
        id: clock

        enabled: ShellState.panelOpen
        precision: SystemClock.Minutes
    }

    StyledText {
        Layout.fillWidth: true
        text: Config.locale.toString(clock.date, Config.timeFormat)
        font.pixelSize: Theme.font.clock
        font.weight: Theme.font.weightLight
    }

    StyledText {
        Layout.fillWidth: true
        text: Config.locale.toString(clock.date, Config.dateFormat)
        color: Theme.colors.textMuted
        font.pixelSize: Theme.font.title
    }
}
