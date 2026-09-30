import QtQuick
import QtQuick.Layouts
import qs
import qs.components
import qs.services

ColumnLayout {
    spacing: Theme.spacing.sm

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
