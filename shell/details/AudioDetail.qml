import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Services.Pipewire
import qs
import qs.components
import qs.services

ColumnLayout {
    id: root

    spacing: Theme.spacing.md

    // Bind every audio node while this view is open (volumes, app names).
    PwObjectTracker {
        objects: [...Audio.sinks, ...Audio.sources, ...Audio.streams]
    }

    DetailHeader {
        Layout.fillWidth: true
        title: I18n.tr("Sound")
    }

    ScrollArea {
        Layout.fillWidth: true
        Layout.fillHeight: true
        spacing: Theme.spacing.xs

        SectionHeader {
            text: I18n.tr("Output")
            bottomPadding: Theme.spacing.xs
        }

        // Default device: the background glides to the newly chosen one.
        Item {
            width: parent.width
            height: sinkRows.implicitHeight

            SlidingHighlight {
                color: Theme.colors.surfaceHigh
                target: {
                    sinkRep.count;
                    return sinkRep.itemAt(Audio.sinks.indexOf(Audio.sink));
                }
            }

            Column {
                id: sinkRows

                width: parent.width
                spacing: Theme.spacing.xs

                Repeater {
                    id: sinkRep

                    model: ScriptModel {
                        values: Audio.sinks
                    }

                    AudioDeviceRow {
                        required property PwNode modelData

                        width: sinkRows.width
                        ownBackground: false
                        node: modelData
                        isDefault: Audio.sink === modelData
                    }
                }
            }
        }

        SectionHeader {
            text: I18n.tr("Input")
            topPadding: Theme.spacing.md
            bottomPadding: Theme.spacing.xs
        }

        // Default device: the background glides to the newly chosen one.
        Item {
            width: parent.width
            height: sourceRows.implicitHeight

            SlidingHighlight {
                color: Theme.colors.surfaceHigh
                target: {
                    sourceRep.count;
                    return sourceRep.itemAt(Audio.sources.indexOf(Audio.source));
                }
            }

            Column {
                id: sourceRows

                width: parent.width
                spacing: Theme.spacing.xs

                Repeater {
                    id: sourceRep

                    model: ScriptModel {
                        values: Audio.sources
                    }

                    AudioDeviceRow {
                        required property PwNode modelData

                        width: sourceRows.width
                        ownBackground: false
                        node: modelData
                        isDefault: Audio.source === modelData
                    }
                }
            }
        }

        StyledText {
            visible: Audio.sources.length === 0
            text: I18n.tr("No input devices")
            color: Theme.colors.textMuted
            leftPadding: Theme.spacing.md
        }

        SectionHeader {
            visible: Audio.streams.length > 0
            text: I18n.tr("Applications")
            topPadding: Theme.spacing.md
            bottomPadding: Theme.spacing.xs
        }

        Repeater {
            model: ScriptModel {
                values: Audio.streams
            }

            AudioDeviceRow {
                required property PwNode modelData

                width: parent.width
                node: modelData
                stream: true
            }
        }
    }
}
