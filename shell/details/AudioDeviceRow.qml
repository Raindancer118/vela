import QtQuick
import QtQuick.Layouts
import Quickshell.Services.Pipewire
import qs
import qs.components
import qs.services

// Device (or app stream) with a volume slider. Clicking a device makes it
// the default.
Rectangle {
    id: root

    required property PwNode node
    property bool isDefault: false
    // Off when a SlidingHighlight draws the default device's background.
    property bool ownBackground: true
    property bool stream: false

    implicitHeight: column.implicitHeight
    radius: Theme.radius.small
    color: isDefault && ownBackground ? Theme.colors.surfaceHigh : "transparent"

    Behavior on color {
        ColorAnim {}
    }

    ColumnLayout {
        id: column

        width: parent.width
        spacing: 0

        ListRow {
            Layout.fillWidth: true
            implicitHeight: Theme.size.listRow - Theme.spacing.sm
            clickable: !root.stream && !root.isDefault
            iconFill: root.isDefault ? 1 : 0
            icon: root.stream ? "music_note" : Audio.deviceIcon(root.node)
            title: root.stream ? Audio.streamName(root.node) : Audio.displayName(root.node)
            subtitle: {
                if (root.stream)
                    return root.node.properties?.["media.name"] ?? "";
                if (root.isDefault)
                    return I18n.tr("Default");
                return root.node.description !== title ? root.node.description : "";
            }
            onClicked: Audio.setDefault(root.node)
        }

        StyledSlider {
            Layout.fillWidth: true
            Layout.leftMargin: Theme.spacing.md
            Layout.rightMargin: Theme.spacing.md
            Layout.bottomMargin: Theme.spacing.md
            implicitHeight: Theme.size.sliderHeightCompact
            enabled: root.node.audio !== null
            icon: Audio.levelIcon(root.node)
            value: root.node.audio?.volume ?? 0
            dimmed: root.node.audio?.muted ?? false
            onMoved: value => Audio.setVolume(root.node, value)
            onIconClicked: Audio.toggleMute(root.node)
        }
    }
}
