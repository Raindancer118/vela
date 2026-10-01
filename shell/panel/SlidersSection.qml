import QtQuick
import QtQuick.Layouts
import qs
import qs.components
import qs.services

// Brightness and volume in one boxed list, like slider rows in the settings.
Card {
    implicitHeight: col.implicitHeight + 2 * Theme.spacing.xs

    ColumnLayout {
    id: col

    anchors.fill: parent
    anchors.margins: Theme.spacing.xs
    anchors.rightMargin: Theme.spacing.md
    spacing: 0

    StyledSlider {
        Layout.fillWidth: true
        visible: Brightness.available
        icon: "light_mode"
        iconInteractive: false
        value: Brightness.value
        onMoved: value => Brightness.set(value)
    }

    StyledSlider {
        Layout.fillWidth: true
        enabled: Audio.sink !== null
        icon: Audio.volumeIcon(Audio.volume, Audio.muted)
        value: Audio.volume
        dimmed: Audio.muted
        onMoved: value => Audio.setVolume(Audio.sink, value)
        onIconClicked: Audio.toggleMute(Audio.sink)
    }
}
}
