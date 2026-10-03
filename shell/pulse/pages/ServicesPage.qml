import QtQuick
import QtQuick.Layouts
import qs
import qs.components
import qs.pulse
import "../Fmt.js" as Fmt
import "../Model.js" as Model

// systemd services of the user and the system: state, autostart, load, and
// start/stop/restart/enable/disable.
Item {
    id: page

    property string filter: "running"
    property string scope: "all"
    readonly property var loc: Qt.locale()
    readonly property var shown: {
        const q = PulseUi.query;
        return Pulse.services.filter(s => {
            if (scope === "user" && !s.user || scope === "system" && s.user)
                return false;
            if (filter === "running" && s.active !== "active" && s.active !== "activating")
                return false;
            if (filter === "failed" && s.active !== "failed")
                return false;
            if (filter === "stopped" && s.active !== "inactive")
                return false;
            return Model.matches(q, [s.unit, s.description]);
        }).sort((a, b) => (a.active === "failed" ? 0 : 1) - (b.active === "failed" ? 0 : 1) || a.unit.localeCompare(b.unit));
    }
    onShownChanged: Model.sync(svcModel, shown.map(s => (s.user ? "u:" : "s:") + s.unit))

    // Load of a service from the app groups (same cgroup).
    function usage(s: var): var {
        return Pulse.apps.find(a => a.unit === s.unit && a.userUnit === s.user) ?? null;
    }

    Component.onCompleted: Pulse.loadServices()

    Timer {
        running: true
        repeat: true
        interval: 5000
        onTriggered: Pulse.loadServices()
    }

    Connections {
        target: Pulse

        function onResult(r: var): void {
            if (r.action.startsWith("unit-"))
                Pulse.loadServices();
        }
    }

    ListModel {
        id: svcModel
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.leftMargin: Theme.spacing.xl
        anchors.rightMargin: Theme.spacing.xl
        anchors.topMargin: Theme.spacing.md
        spacing: Theme.spacing.sm

        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.spacing.sm

            Repeater {
                model: [
                    {
                        k: "running",
                        t: I18n.tr("Running")
                    },
                    {
                        k: "failed",
                        t: I18n.tr("Failed")
                    },
                    {
                        k: "stopped",
                        t: I18n.tr("Stopped")
                    },
                    {
                        k: "all",
                        t: I18n.tr("All")
                    }
                ]

                Rectangle {
                    required property var modelData
                    readonly property bool on: page.filter === modelData.k
                    readonly property int count: modelData.k === "failed" ? Pulse.services.filter(s => s.active === "failed").length : 0

                    implicitHeight: 30
                    implicitWidth: fRow.implicitWidth + 2 * Theme.spacing.md
                    radius: 15
                    color: on ? Theme.colors.selected : Theme.withAlpha(Theme.colors.text, 0.05)
                    border.width: on ? 1 : 0
                    border.color: Theme.colors.selectedRing
                    Behavior on color {
                        ColorAnim {}
                    }

                    Clickable {
                        radius: 15
                        onClicked: page.filter = parent.modelData.k
                    }

                    RowLayout {
                        id: fRow

                        anchors.centerIn: parent
                        spacing: Theme.spacing.xs

                        StyledText {
                            text: parent.parent.modelData.t
                            font.pixelSize: Theme.font.small + 1
                        }

                        Rectangle {
                            visible: parent.parent.count > 0
                            implicitWidth: 18
                            implicitHeight: 18
                            radius: 9
                            color: Theme.pulse.crit

                            StyledText {
                                anchors.centerIn: parent
                                text: parent.parent.parent.count
                                color: Theme.colors.background
                                font.pixelSize: Theme.font.small - 2
                                font.weight: Theme.font.weightSemiBold
                            }
                        }
                    }
                }
            }

            Item {
                Layout.fillWidth: true
            }

            Repeater {
                model: [
                    {
                        k: "all",
                        t: I18n.tr("User and system")
                    },
                    {
                        k: "user",
                        t: I18n.tr("User")
                    },
                    {
                        k: "system",
                        t: I18n.tr("System")
                    }
                ]

                Rectangle {
                    required property var modelData
                    readonly property bool on: page.scope === modelData.k

                    implicitHeight: 30
                    implicitWidth: sText.implicitWidth + 2 * Theme.spacing.md
                    radius: 15
                    color: on ? Theme.colors.selected : "transparent"
                    border.width: on ? 1 : 0
                    border.color: Theme.colors.selectedRing

                    Clickable {
                        radius: 15
                        onClicked: page.scope = parent.modelData.k
                    }

                    StyledText {
                        id: sText

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
            model: svcModel
            spacing: 2
            boundsBehavior: Flickable.StopAtBounds

            add: Transition {
                Anim {
                    property: "opacity"
                    from: 0
                    to: 1
                }
            }
            remove: Transition {
                Anim {
                    property: "opacity"
                    to: 0
                    duration: Theme.anim.fast
                }
            }
            displaced: Transition {
                SpringAnim {
                    property: "y"
                    duration: Theme.anim.normal
                }
                // An interrupted add/remove must not leave the row half visible.
                Anim {
                    properties: "opacity,scale"
                    to: 1
                }
                Anim {
                    property: "x"
                    to: 0
                }
            }

            delegate: Rectangle {
                id: row

                required property string key
                readonly property var s: page.shown.find(x => (x.user ? "u:" : "s:") + x.unit === key) ?? null
                readonly property var use: s ? page.usage(s) : null
                readonly property bool running: s?.active === "active" || s?.active === "activating"
                readonly property bool failed: s?.active === "failed"
                readonly property string busy: Pulse.busy[s?.unit ?? ""] ?? ""

                width: ListView.view.width
                height: 54
                radius: Theme.radius.small
                color: rowArea.containsMouse ? Theme.colors.hover : "transparent"
                opacity: busy !== "" ? 0.6 : 1
                Behavior on opacity {
                    Anim {}
                }

                MouseArea {
                    id: rowArea

                    anchors.fill: parent
                    hoverEnabled: true
                    acceptedButtons: Qt.NoButton
                }

                RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: Theme.spacing.md
                    anchors.rightMargin: Theme.spacing.sm
                    spacing: Theme.spacing.md

                    // State dot; breathes while running, red when failed.
                    Rectangle {
                        implicitWidth: 10
                        implicitHeight: 10
                        radius: 5
                        color: row.failed ? Theme.pulse.crit : row.running ? Theme.pulse.ok : Theme.colors.textDisabled
                        SequentialAnimation on opacity {
                            running: row.running && Theme.anim.normal > 0
                            loops: Animation.Infinite
                            NumberAnimation {
                                to: 0.45
                                duration: 1400
                                easing.type: Easing.InOutSine
                            }
                            NumberAnimation {
                                to: 1
                                duration: 1400
                                easing.type: Easing.InOutSine
                            }
                        }
                    }

                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 0

                        StyledText {
                            Layout.fillWidth: true
                            text: row.s?.description || row.s?.unit || ""
                            font.weight: Theme.font.weightMedium
                        }

                        StyledText {
                            Layout.fillWidth: true
                            text: (row.s?.unit ?? "") + "  ·  " + (row.s?.user ? I18n.tr("user") : I18n.tr("system")) + "  ·  " + (row.s?.sub ?? "")
                            color: Theme.colors.textMuted
                            font.pixelSize: Theme.font.small
                        }
                    }

                    // Autostart; static/indirect units can't be switched.
                    Item {
                        Layout.preferredWidth: 150
                        Layout.fillHeight: true

                        RowLayout {
                            anchors.right: parent.right
                            anchors.verticalCenter: parent.verticalCenter
                            spacing: Theme.spacing.sm

                            StyledText {
                                visible: (row.s?.enabled ?? "") !== ""
                                text: row.s?.enabled === "enabled" || row.s?.enabled === "disabled" ? I18n.tr("Autostart") : (row.s?.enabled ?? "")
                                color: Theme.colors.textMuted
                                font.pixelSize: Theme.font.small
                            }

                            Switch {
                                id: autoSwitch

                                visible: row.s?.enabled === "enabled" || row.s?.enabled === "disabled"
                                checked: row.s?.enabled === "enabled"
                                onToggled: Pulse.unitAction(row.s.unit, row.s.user, row.s.enabled === "enabled" ? "disable" : "enable")
                            }
                        }
                    }

                    StyledText {
                        Layout.preferredWidth: 70
                        horizontalAlignment: Text.AlignRight
                        text: row.use ? Fmt.percent(row.use.cpu, page.loc) : ""
                        font.features: { "tnum": 1 }
                    }

                    StyledText {
                        Layout.preferredWidth: 80
                        horizontalAlignment: Text.AlignRight
                        text: row.use ? Fmt.bytes(row.use.mem, page.loc) : ""
                        font.features: { "tnum": 1 }
                    }

                    Row {
                        Layout.preferredWidth: 100
                        layoutDirection: Qt.RightToLeft
                        spacing: 2
                        opacity: rowArea.containsMouse || row.failed ? 1 : 0.0
                        Behavior on opacity {
                            Anim {
                                duration: Theme.anim.fast
                            }
                        }

                        IconButton {
                            visible: !row.running
                            implicitWidth: 30
                            implicitHeight: 30
                            icon: "play_arrow"
                            iconSize: Theme.icon.small

                            Tip {
                                text: I18n.tr("Start")
                                shown: parent.hovered
                            }
                            onClicked: Pulse.unitAction(row.s.unit, row.s.user, "start")
                        }

                        IconButton {
                            visible: row.running || row.failed
                            implicitWidth: 30
                            implicitHeight: 30
                            icon: "restart_alt"
                            iconSize: Theme.icon.small

                            Tip {
                                text: I18n.tr("Restart")
                                shown: parent.hovered
                            }
                            onClicked: Pulse.unitAction(row.s.unit, row.s.user, "restart")
                        }

                        IconButton {
                            visible: row.running
                            implicitWidth: 30
                            implicitHeight: 30
                            icon: "stop_circle"
                            iconSize: Theme.icon.small

                            Tip {
                                text: I18n.tr("Stop")
                                shown: parent.hovered
                            }
                            onClicked: {
                                const s = row.s;
                                if (s.user)
                                    Pulse.unitAction(s.unit, true, "stop");
                                else
                                    PulseUi.ask(I18n.tr("Stop %1?", s.description || s.unit), I18n.tr("It is part of the system. Things that depend on it may stop working until it runs again."), I18n.tr("Stop"), true, () => Pulse.unitAction(s.unit, false, "stop"));
                            }
                        }

                        IconButton {
                            visible: row.failed
                            implicitWidth: 30
                            implicitHeight: 30
                            icon: "check"
                            iconSize: Theme.icon.small

                            Tip {
                                text: I18n.tr("Clear error")
                                shown: parent.hovered
                            }
                            onClicked: Pulse.unitAction(row.s.unit, row.s.user, "reset-failed")
                        }

                    }
                }
            }

            StyledText {
                anchors.centerIn: parent
                visible: svcModel.count === 0
                text: Pulse.services.length === 0 ? I18n.tr("Loading services…") : I18n.tr("No services here")
                color: Theme.colors.textMuted
            }
        }
    }
}
