import QtQuick
import QtQuick.Layouts
import qs
import qs.components
import qs.notifications
import qs.services

Rectangle {
    id: root

    color: Theme.colors.panel
    radius: Theme.radius.panel
    border.width: Theme.size.border
    border.color: Theme.colors.outline

    // Swallow clicks so they don't reach the "click outside" area.
    MouseArea {
        anchors.fill: parent
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: Theme.size.panelPadding
        spacing: Theme.spacing.lg

        HeaderSection {
            Layout.fillWidth: true
        }

        ClockSection {
            Layout.fillWidth: true
            visible: !Config.clockOnBackdrop
        }

        Divider {
            Layout.fillWidth: true
            visible: !Config.clockOnBackdrop
        }

        MediaPlayer {
            Layout.fillWidth: true
            visible: Media.available && Config.mediaPlayerPosition === "top"
        }

        SlidersSection {
            Layout.fillWidth: true
        }

        QuickSettingsGrid {
            Layout.fillWidth: true
        }

        MediaPlayer {
            Layout.fillWidth: true
            visible: Media.available && Config.mediaPlayerPosition === "tiles"
        }

        Divider {
            Layout.fillWidth: true
        }

        NotificationList {
            Layout.fillWidth: true
            Layout.fillHeight: true
        }

        // Low screens (laptop at 1.5x): notifications need the room more.
        ClaudeUsageSection {
            Layout.fillWidth: true
            visible: Config.claudeUsage && accounts.length > 0 && root.height >= Theme.size.claudeUsageMinPanelHeight && !bottomPlayer.visible
        }

        // In place of the Claude card while something plays.
        MediaPlayer {
            id: bottomPlayer

            Layout.fillWidth: true
            visible: Media.available && Config.mediaPlayerPosition === "bottom"
        }
    }
}
