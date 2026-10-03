import QtQuick
import QtQuick.Layouts
import qs
import qs.components
import qs.pulse
import "../Model.js" as Model

// Timeline of what happened while Pulse was open: apps starting and
// closing, crashes, out-of-memory kills, hangs, failed services, actions.
Item {
    id: page

    property string filter: "all"
    readonly property var groups: ({
            all: null,
            problems: ["crashed", "oom", "failed", "not-responding", "action-failed"],
            apps: ["started", "closed", "ended", "restarted", "responding"]
        })
    readonly property var shown: Pulse.events.filter(e => (groups[filter] === null || groups[filter].indexOf(e.kind) >= 0) && Model.matches(PulseUi.query, [e.name, e.detail, Words.event(e)]))

    ColumnLayout {
        anchors.fill: parent
        anchors.leftMargin: Theme.spacing.xl
        anchors.rightMargin: Theme.spacing.xl
        anchors.topMargin: Theme.spacing.md
        spacing: Theme.spacing.md

        RowLayout {
            spacing: Theme.spacing.sm

            Repeater {
                model: [
                    {
                        k: "all",
                        t: I18n.tr("Everything")
                    },
                    {
                        k: "problems",
                        t: I18n.tr("Problems")
                    },
                    {
                        k: "apps",
                        t: I18n.tr("Apps")
                    }
                ]

                Rectangle {
                    required property var modelData
                    readonly property bool on: page.filter === modelData.k

                    implicitHeight: 30
                    implicitWidth: t.implicitWidth + 2 * Theme.spacing.md
                    radius: 15
                    color: on ? Theme.colors.selected : Theme.withAlpha(Theme.colors.text, 0.05)
                    border.width: on ? 1 : 0
                    border.color: Theme.colors.selectedRing

                    Clickable {
                        radius: 15
                        onClicked: page.filter = parent.modelData.k
                    }

                    StyledText {
                        id: t

                        anchors.centerIn: parent
                        text: parent.modelData.t
                        font.pixelSize: Theme.font.small + 1
                    }
                }
            }
        }

        ListView {
            id: list

            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            model: page.shown
            boundsBehavior: Flickable.StopAtBounds

            delegate: Item {
                id: ev

                required property var modelData
                required property int index
                readonly property color tint: Words.eventColor(modelData.kind)
                // A heading when the day changes.
                readonly property bool newDay: index === 0 || new Date(page.shown[index - 1].t).toDateString() !== new Date(modelData.t).toDateString()

                width: ListView.view.width
                height: 56 + (newDay ? 30 : 0)

                StyledText {
                    visible: ev.newDay
                    y: 6
                    text: Qt.formatDate(new Date(ev.modelData.t), Qt.locale(), Locale.LongFormat)
                    color: Theme.colors.textMuted
                    font.pixelSize: Theme.font.small
                    font.weight: Theme.font.weightMedium
                }

                Item {
                    y: ev.newDay ? 30 : 0
                    width: parent.width
                    height: 56

                    // The time line.
                    Rectangle {
                        x: 74 + 15
                        width: 2
                        height: parent.height
                        color: Theme.colors.divider
                    }

                    StyledText {
                        width: 66
                        anchors.verticalCenter: parent.verticalCenter
                        horizontalAlignment: Text.AlignRight
                        text: Qt.formatTime(new Date(ev.modelData.t), "HH:mm:ss")
                        color: Theme.colors.textMuted
                        font.pixelSize: Theme.font.small
                        font.features: { "tnum": 1 }
                    }

                    Rectangle {
                        x: 74
                        anchors.verticalCenter: parent.verticalCenter
                        width: 32
                        height: 32
                        radius: 16
                        color: Theme.colors.background
                        border.width: 2
                        border.color: Theme.withAlpha(ev.tint, 0.6)

                        MaterialIcon {
                            anchors.centerIn: parent
                            icon: Words.eventIcon(ev.modelData.kind)
                            size: Theme.icon.small
                            color: ev.tint
                        }
                    }

                    RowLayout {
                        x: 74 + 32 + Theme.spacing.md
                        width: parent.width - x
                        height: parent.height
                        spacing: Theme.spacing.md

                        AppIcon {
                            visible: ev.modelData.icon !== ""
                            size: 24
                            icon: ev.modelData.icon
                            name: ev.modelData.name
                        }

                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 0

                            StyledText {
                                Layout.fillWidth: true
                                text: Words.event(ev.modelData)
                                font.weight: Theme.font.weightMedium
                            }

                            StyledText {
                                Layout.fillWidth: true
                                visible: text !== ""
                                text: ev.modelData.kind === "action-failed" ? "" : ev.modelData.detail
                                color: Theme.colors.textMuted
                                font.pixelSize: Theme.font.small
                            }
                        }

                        PillButton {
                            visible: ev.modelData.key !== "" && Pulse.app(ev.modelData.key) !== null
                            text: I18n.tr("Show")
                            onClicked: PulseUi.showApp(ev.modelData.key)
                        }
                    }
                }
            }

            ColumnLayout {
                anchors.centerIn: parent
                visible: page.shown.length === 0
                spacing: Theme.spacing.sm

                MaterialIcon {
                    Layout.alignment: Qt.AlignHCenter
                    icon: "schedule"
                    size: 40
                    color: Theme.colors.textDisabled
                }

                StyledText {
                    Layout.alignment: Qt.AlignHCenter
                    text: I18n.tr("Nothing happened yet")
                    color: Theme.colors.textMuted
                }

                StyledText {
                    Layout.alignment: Qt.AlignHCenter
                    text: I18n.tr("Apps that start, close, crash or hang show up here.")
                    color: Theme.colors.textDisabled
                    font.pixelSize: Theme.font.small
                }
            }
        }
    }
}
