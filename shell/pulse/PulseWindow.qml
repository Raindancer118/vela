import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Hyprland
import qs
import qs.components
import "Fmt.js" as Fmt

// The task manager window: sidebar, header, pages, and the overlays
// (details flyout, run dialog, confirmations, toasts).
FloatingWindow {
    id: win

    title: I18n.tr("Pulse — Task manager")
    implicitWidth: 1280
    implicitHeight: 820
    minimumSize: Qt.size(900, 600)
    color: "transparent"
    // Closed (its X, Super+Q, `vela pulse close`): end Pulse instead of
    // lingering without a window.
    onVisibleChanged: if (!visible)
        Qt.quit()

    // Hyprland: focus this window (vela pulse while it is open).
    function raise(): void {
        Hyprland.dispatch(`hl.dsp.focus({ window = "title:^${title}$" })`);
    }

    readonly property var nav: [
        {
            page: "overview",
            icon: "pulse_heart",
            label: I18n.tr("Overview")
        },
        {
            page: "processes",
            icon: "apps",
            label: I18n.tr("Apps & processes")
        },
        {
            page: "performance",
            icon: "speed",
            label: I18n.tr("Performance")
        },
        {
            page: "diagnosis",
            icon: "shield",
            label: I18n.tr("Diagnosis")
        },
        {
            page: "services",
            icon: "services",
            label: I18n.tr("Services")
        },
        {
            page: "activity",
            icon: "schedule",
            label: I18n.tr("Activity")
        }
    ]

    Rectangle {
        id: bg

        anchors.fill: parent
        color: Theme.colors.panel
        opacity: 0
        Component.onCompleted: opacity = 1
        Behavior on opacity {
            Anim {
                duration: Theme.anim.slow
            }
        }
    }

    Item {
        id: body

        anchors.fill: parent
        focus: true

        // Opening: the content settles in from slightly below.
        transform: Translate {
            id: openShift

            y: Theme.anim.slideShort
            Component.onCompleted: y = 0
            Behavior on y {
                Anim {
                    duration: Theme.anim.slow
                    easing.bezierCurve: Theme.anim.emphasizedDecel
                }
            }
        }

        Keys.onPressed: event => {
            if (!PulseUi.confirm && !PulseUi.runOpen && PulseUi.hotkey(event)) {
                event.accepted = true;
                return;
            }
            const ctrl = event.modifiers & Qt.ControlModifier;
            if (event.key === Qt.Key_Escape) {
                if (PulseUi.confirm)
                    PulseUi.confirm = null;
                else if (PulseUi.runOpen)
                    PulseUi.runOpen = false;
                else if (PulseUi.detailKey !== "")
                    PulseUi.detailKey = "";
                else if (PulseUi.query !== "")
                    PulseUi.query = "";
                else
                    return;
            } else if (ctrl && event.key >= Qt.Key_1 && event.key <= Qt.Key_6) {
                PulseUi.show(PulseUi.pages[event.key - Qt.Key_1]);
            } else if (ctrl && event.key === Qt.Key_F) {
                header.focusSearch();
            } else if (ctrl && event.key === Qt.Key_N) {
                PulseUi.runOpen = true;
            } else if (ctrl && event.key === Qt.Key_W) {
                Qt.quit();
            } else {
                return;
            }
            event.accepted = true;
        }

        RowLayout {
            anchors.fill: parent
            spacing: 0

            // Sidebar
            Item {
                Layout.fillHeight: true
                Layout.preferredWidth: Theme.pulse.sidebarWidth

                ColumnLayout {
                    anchors.fill: parent
                    anchors.margins: Theme.spacing.md
                    spacing: Theme.spacing.xs

                    RowLayout {
                        Layout.fillWidth: true
                        Layout.leftMargin: Theme.spacing.sm
                        Layout.topMargin: Theme.spacing.sm
                        Layout.bottomMargin: Theme.spacing.lg
                        spacing: Theme.spacing.sm

                        Rectangle {
                            implicitWidth: 34
                            implicitHeight: 34
                            radius: 11
                            color: Theme.colors.accentChip

                            MaterialIcon {
                                id: logo

                                anchors.centerIn: parent
                                icon: "pulse_heart"
                                size: 20
                                color: Theme.colors.primary
                            }
                        }

                        ColumnLayout {
                            spacing: 0

                            StyledText {
                                text: "Pulse"
                                font.pixelSize: Theme.font.large
                                font.weight: Theme.font.weightSemiBold
                            }

                            StyledText {
                                Layout.fillWidth: true
                                text: Pulse.hello.hostname ?? ""
                                color: Theme.colors.textMuted
                                font.pixelSize: Theme.font.small
                            }
                        }

                        Item {
                            Layout.fillWidth: true
                        }

                        IconButton {
                            icon: "settings"
                            iconSize: Theme.icon.small
                            iconColor: Theme.colors.textMuted
                            onClicked: Quickshell.execDetached([Quickshell.env("VELA_BIN") || "vela", "settings", "pulse"])

                            Tip {
                                text: I18n.tr("Settings")
                                shown: parent.hovered
                            }
                        }
                    }

                    Item {
                        Layout.fillWidth: true
                        Layout.preferredHeight: navColumn.implicitHeight

                        SlidingHighlight {
                            target: navRepeater.itemAt(win.nav.findIndex(n => n.page === PulseUi.page))
                            color: Theme.colors.selected
                            border.width: Theme.size.border
                            border.color: Theme.colors.selectedRing
                        }

                        Column {
                            id: navColumn

                            width: parent.width
                            spacing: 2

                            Repeater {
                                id: navRepeater

                                model: win.nav

                                Item {
                                    id: navItem

                                    required property var modelData
                                    required property int index
                                    readonly property bool current: PulseUi.page === modelData.page
                                    readonly property int badge: {
                                        if (modelData.page === "diagnosis")
                                            return Pulse.findings.filter(f => f.severity !== "info").length;
                                        if (modelData.page === "services")
                                            return Pulse.failedUnits.length;
                                        return 0;
                                    }

                                    width: navColumn.width
                                    height: 40

                                    Clickable {
                                        radius: Theme.radius.small
                                        onClicked: PulseUi.show(navItem.modelData.page)
                                    }

                                    RowLayout {
                                        anchors.fill: parent
                                        anchors.leftMargin: Theme.spacing.md
                                        anchors.rightMargin: Theme.spacing.md
                                        spacing: Theme.spacing.md

                                        MaterialIcon {
                                            icon: navItem.modelData.icon
                                            size: Theme.icon.normal
                                            color: navItem.current ? Theme.colors.primary : Theme.colors.textMuted
                                            Behavior on color {
                                                ColorAnim {}
                                            }
                                        }

                                        StyledText {
                                            Layout.fillWidth: true
                                            text: navItem.modelData.label
                                            font.weight: navItem.current ? Theme.font.weightMedium : Theme.font.weightNormal
                                        }

                                        Rectangle {
                                            visible: navItem.badge > 0
                                            implicitWidth: Math.max(20, badgeText.implicitWidth + 10)
                                            implicitHeight: 20
                                            radius: 10
                                            color: Theme.pulse.warn
                                            scale: visible ? 1 : 0
                                            Behavior on scale {
                                                SpringAnim {}
                                            }

                                            StyledText {
                                                id: badgeText

                                                anchors.centerIn: parent
                                                text: navItem.badge
                                                color: Theme.colors.background
                                                font.pixelSize: Theme.font.small - 1
                                                font.weight: Theme.font.weightSemiBold
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    Item {
                        Layout.fillHeight: true
                    }

                    // Live mini stats at the bottom of the sidebar.
                    Repeater {
                        model: [
                            {
                                key: "cpu",
                                label: I18n.tr("CPU"),
                                color: Theme.pulse.cpu
                            },
                            {
                                key: "mem",
                                label: I18n.tr("Memory"),
                                color: Theme.pulse.memory
                            }
                        ]

                        Item {
                            required property var modelData

                            Layout.fillWidth: true
                            Layout.preferredHeight: 46

                            Clickable {
                                radius: Theme.radius.small
                                onClicked: PulseUi.showPerf(parent.modelData.key === "cpu" ? "cpu" : "memory")
                            }

                            Graph {
                                anchors.fill: parent
                                anchors.margins: 2
                                values: Pulse.series(parent.modelData.key, 62)
                                max: 100
                                points: 60
                                grid: false
                                hoverable: false
                                color: parent.modelData.color
                                opacity: 0.55
                            }

                            RowLayout {
                                anchors.fill: parent
                                anchors.leftMargin: Theme.spacing.sm
                                anchors.rightMargin: Theme.spacing.sm

                                StyledText {
                                    text: parent.parent.modelData.label
                                    color: Theme.colors.textMuted
                                    font.pixelSize: Theme.font.small
                                }

                                Item {
                                    Layout.fillWidth: true
                                }

                                Counter {
                                    value: {
                                        const s = Pulse.series(parent.parent.modelData.key, 1);
                                        return s.length > 0 ? s[0] : 0;
                                    }
                                    format: v => Fmt.percent(v, Qt.locale(), 0)
                                    font.weight: Theme.font.weightMedium
                                }
                            }
                        }
                    }
                }

                Rectangle {
                    anchors.right: parent.right
                    width: 1
                    height: parent.height
                    color: Theme.colors.divider
                }
            }

            // Content
            ColumnLayout {
                Layout.fillWidth: true
                Layout.fillHeight: true
                spacing: 0

                Header {
                    id: header

                    Layout.fillWidth: true
                }

                Item {
                    id: host

                    Layout.fillWidth: true
                    Layout.fillHeight: true

                    Repeater {
                        model: PulseUi.pages

                        Loader {
                            id: slot

                            required property string modelData
                            readonly property bool current: PulseUi.page === modelData
                            property bool leaving: false

                            anchors.fill: parent
                            active: current || leaving
                            visible: active
                            opacity: current ? 1 : 0
                            z: current ? 1 : 0
                            source: "pages/" + modelData.charAt(0).toUpperCase() + modelData.slice(1) + "Page.qml"
                            onCurrentChanged: {
                                if (!current) {
                                    leaving = true;
                                    leaveTimer.restart();
                                }
                            }
                            transform: Translate {
                                y: slot.current ? 0 : Theme.anim.slideShort
                                Behavior on y {
                                    Anim {
                                        easing.bezierCurve: Theme.anim.emphasizedDecel
                                    }
                                }
                            }
                            Behavior on opacity {
                                Anim {}
                            }

                            Timer {
                                id: leaveTimer

                                interval: Theme.anim.normal + 30
                                onTriggered: slot.leaving = false
                            }
                        }
                    }
                }
            }
        }

        Flyout {
            id: flyout

            anchors.top: parent.top
            anchors.bottom: parent.bottom
            anchors.right: parent.right
        }

        RunDialog {
            anchors.fill: parent
        }

        ContextMenu {
            anchors.fill: parent
        }

        ConfirmDialog {
            anchors.fill: parent
        }

        Toasts {
            anchors.bottom: parent.bottom
            anchors.horizontalCenter: parent.horizontalCenter
            anchors.horizontalCenterOffset: Theme.pulse.sidebarWidth / 2
            anchors.bottomMargin: Theme.spacing.xl
        }

        Rectangle {
            visible: !Pulse.connected
            anchors.centerIn: parent
            width: 320
            height: 64
            radius: Theme.radius.card
            color: Theme.colors.surfaceHigh

            StyledText {
                anchors.centerIn: parent
                text: I18n.tr("Connecting to the system…")
                color: Theme.colors.textMuted
            }
        }
    }
}
