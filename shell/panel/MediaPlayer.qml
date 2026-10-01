import QtQuick
import QtQuick.Effects
import QtQuick.Layouts
import qs
import qs.components
import qs.services

// Mini player for Media.player (vela settings → Panel → Mini player):
// album art, title and artist, previous / play-pause / next, and a progress
// bar to seek. Optionally the album art blurred behind the whole card.
Card {
    id: root

    readonly property var player: Media.player
    readonly property real length: player ? player.length : 0
    readonly property bool coverBackground: Config.mediaPlayerCoverBackground && art.status === Image.Ready

    visible: Media.available
    implicitHeight: col.implicitHeight + 2 * Theme.spacing.md

    // Album art, blurred and darkened behind everything, clipped to the card's corners.
    Item {
        id: background

        anchors.fill: parent
        opacity: root.coverBackground ? 1 : 0
        visible: opacity > 0
        layer.enabled: visible
        layer.effect: MultiEffect {
            maskEnabled: true
            maskSource: mask
        }

        Behavior on opacity {
            Anim {}
        }

        Image {
            id: art

            anchors.fill: parent
            source: root.player ? root.player.trackArtUrl : ""
            fillMode: Image.PreserveAspectCrop
            asynchronous: true
            visible: false
        }

        MultiEffect {
            anchors.fill: parent
            source: art
            blurEnabled: true
            blur: 1
            blurMax: 48
            saturation: 0.2
            brightness: Theme.va.light ? 0.2 : -0.35
        }

        // Keeps text readable on light covers.
        Rectangle {
            anchors.fill: parent
            color: Theme.withAlpha(Theme.colors.background, 0.62)
        }
    }

    Rectangle {
        id: mask

        anchors.fill: parent
        radius: root.radius
        visible: false
        layer.enabled: true
    }

    ColumnLayout {
        id: col

        anchors.fill: parent
        anchors.margins: Theme.spacing.md
        spacing: Theme.spacing.sm

        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.spacing.md

            // Cover with rounded corners; a note when the track has none.
            Item {
                visible: Config.mediaPlayerCover
                Layout.preferredWidth: Theme.size.mediaCover
                Layout.preferredHeight: Theme.size.mediaCover

                Rectangle {
                    anchors.fill: parent
                    radius: Theme.radius.small
                    color: Theme.colors.surfaceHigh

                    MaterialIcon {
                        anchors.centerIn: parent
                        icon: "music_note"
                        color: Theme.colors.textMuted
                        visible: thumb.status !== Image.Ready
                    }
                }

                Image {
                    id: thumb

                    anchors.fill: parent
                    source: root.player ? root.player.trackArtUrl : ""
                    sourceSize: Qt.size(Theme.size.mediaCover * 2, Theme.size.mediaCover * 2)
                    fillMode: Image.PreserveAspectCrop
                    asynchronous: true
                    layer.enabled: true
                    layer.effect: MultiEffect {
                        maskEnabled: true
                        maskSource: thumbMask
                    }
                }

                Rectangle {
                    id: thumbMask

                    anchors.fill: parent
                    radius: Theme.radius.small
                    visible: false
                    layer.enabled: true
                }

                Clickable {
                    enabled: root.player !== null && root.player.canRaise
                    radius: Theme.radius.small
                    onClicked: root.player.raise()
                }
            }

            ColumnLayout {
                Layout.fillWidth: true
                spacing: 0

                SwapText {
                    Layout.fillWidth: true
                    value: root.player ? root.player.trackTitle : ""
                    font.weight: Theme.font.weightSemiBold
                    elide: Text.ElideRight
                }

                StyledText {
                    Layout.fillWidth: true
                    text: root.player ? root.player.trackArtist : ""
                    visible: text !== ""
                    color: Theme.colors.textMuted
                    font.pixelSize: Theme.font.small
                    elide: Text.ElideRight
                }
            }

            IconButton {
                icon: "skip_previous"
                enabled: root.player !== null && root.player.canGoPrevious
                onClicked: root.player.previous()
            }

            IconButton {
                icon: root.player && root.player.isPlaying ? "pause" : "play_arrow"
                tonal: true
                enabled: root.player !== null && root.player.canTogglePlaying
                onClicked: root.player.togglePlaying()
            }

            IconButton {
                icon: "skip_next"
                enabled: root.player !== null && root.player.canGoNext
                onClicked: root.player.next()
            }
        }

        // Progress: elapsed, a thin bar to click or drag, remaining length.
        RowLayout {
            Layout.fillWidth: true
            visible: Config.mediaPlayerProgress && root.length > 0
            spacing: Theme.spacing.sm

            component TimeLabel: StyledText {
                color: Theme.colors.textMuted
                font.pixelSize: Theme.font.small
                font.features: ({
                        "tnum": 1
                    })
            }

            TimeLabel {
                text: Media.time(bar.shown * root.length)
            }

            Item {
                id: bar

                readonly property bool canSeek: root.player !== null && root.player.canSeek
                property real dragValue: 0
                readonly property real shown: seek.pressed ? dragValue : root.length > 0 && root.player ? Math.min(1, root.player.position / root.length) : 0

                function valueAt(x: real): real {
                    return Math.max(0, Math.min(1, x / width));
                }

                Layout.fillWidth: true
                Layout.preferredHeight: Theme.size.sliderKnob

                Rectangle {
                    anchors.verticalCenter: parent.verticalCenter
                    width: parent.width
                    height: Theme.size.sliderTrack
                    radius: height / 2
                    color: Theme.colors.surfaceHighest

                    Rectangle {
                        width: parent.width * bar.shown
                        height: parent.height
                        radius: height / 2
                        color: Theme.colors.primary
                    }
                }

                Rectangle {
                    anchors.verticalCenter: parent.verticalCenter
                    x: bar.shown * bar.width - width / 2
                    width: Theme.size.mediaKnob
                    height: width
                    radius: width / 2
                    color: Theme.colors.knob
                    visible: bar.canSeek
                    scale: seek.pressed || seek.containsMouse ? 1 : 0

                    Behavior on scale {
                        Anim {
                            duration: Theme.anim.fast
                        }
                    }
                }

                MouseArea {
                    id: seek

                    anchors.fill: parent
                    enabled: bar.canSeek
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    preventStealing: true
                    onPressed: mouse => bar.dragValue = bar.valueAt(mouse.x)
                    onPositionChanged: mouse => {
                        if (pressed)
                            bar.dragValue = bar.valueAt(mouse.x);
                    }
                    onReleased: root.player.position = bar.dragValue * root.length
                }
            }

            TimeLabel {
                text: Media.time(root.length)
            }
        }
    }
}
