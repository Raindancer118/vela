import QtQuick
import QtQuick.Layouts
import Quickshell
import qs
import qs.components
import "Model.js" as Model

// "Run new task": an app from the menu, or any command (optionally in the
// terminal). Typing filters the apps; Enter runs the selection or the command.
Scrim {
    id: root

    property int sel: 0
    property bool terminal: false
    readonly property string q: field.text
    readonly property var hits: Pulse.launchable.map(a => ({
                a: a,
                s: Math.max(Model.score(q, a.name), Model.score(q, a.id) * 0.9)
            })).filter(x => x.s > 0).sort((x, y) => y.s - x.s || x.a.name.localeCompare(y.a.name)).slice(0, 6).map(x => x.a)

    open: PulseUi.runOpen
    cardWidth: 540
    onDismissed: PulseUi.runOpen = false
    onOpenChanged: {
        if (open) {
            Pulse.loadLaunchable();
            field.text = "";
            sel = 0;
            field.input.forceActiveFocus();
        }
    }
    onQChanged: sel = 0

    function go(): void {
        if (q.trim() === "")
            return;
        if (sel < hits.length)
            Pulse.launch(hits[sel].id);
        else
            Pulse.run(q, terminal);
        PulseUi.runOpen = false;
    }

    ColumnLayout {
        width: parent.width
        spacing: Theme.spacing.md

        RowLayout {
            spacing: Theme.spacing.sm

            MaterialIcon {
                icon: "run"
                color: Theme.colors.primary
            }

            StyledText {
                text: I18n.tr("Run new task")
                font.pixelSize: Theme.font.large
                font.weight: Theme.font.weightSemiBold
            }
        }

        TextField {
            id: field

            Layout.fillWidth: true
            placeholder: I18n.tr("App name or command, e.g. firefox --private-window")
            onAccepted: root.go()
            input.Keys.onDownPressed: root.sel = Math.min(root.hits.length, root.sel + 1)
            input.Keys.onUpPressed: root.sel = Math.max(0, root.sel - 1)
            input.Keys.onEscapePressed: PulseUi.runOpen = false
        }

        Column {
            Layout.fillWidth: true
            visible: root.q.trim() !== ""
            spacing: 2

            Repeater {
                model: root.hits

                Rectangle {
                    required property var modelData
                    required property int index

                    width: parent.width
                    height: 40
                    radius: Theme.radius.small
                    color: root.sel === index ? Theme.colors.selected : "transparent"
                    Behavior on color {
                        ColorAnim {
                            duration: Theme.anim.fast
                        }
                    }

                    Clickable {
                        radius: parent.radius
                        onClicked: {
                            root.sel = parent.index;
                            root.go();
                        }
                    }

                    RowLayout {
                        anchors.fill: parent
                        anchors.leftMargin: Theme.spacing.sm
                        anchors.rightMargin: Theme.spacing.sm
                        spacing: Theme.spacing.md

                        AppIcon {
                            icon: parent.parent.modelData.icon ?? ""
                            name: parent.parent.modelData.name
                            size: 24
                        }

                        StyledText {
                            text: parent.parent.modelData.name
                        }

                        StyledText {
                            Layout.fillWidth: true
                            text: parent.parent.modelData.comment ?? ""
                            color: Theme.colors.textMuted
                            font.pixelSize: Theme.font.small
                        }
                    }
                }
            }

            // The typed text as a command.
            Rectangle {
                width: parent.width
                height: 40
                radius: Theme.radius.small
                color: root.sel === root.hits.length ? Theme.colors.selected : "transparent"

                Clickable {
                    radius: parent.radius
                    onClicked: {
                        root.sel = root.hits.length;
                        root.go();
                    }
                }

                RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: Theme.spacing.sm
                    spacing: Theme.spacing.md

                    MaterialIcon {
                        icon: "terminal"
                        color: Theme.colors.textMuted
                    }

                    StyledText {
                        Layout.fillWidth: true
                        text: I18n.tr("Run “%1”", root.q.trim())
                    }
                }
            }
        }

        RowLayout {
            Layout.fillWidth: true

            Switch {
                id: termSwitch

                checked: root.terminal
                onToggled: root.terminal = !root.terminal
            }

            StyledText {
                Layout.fillWidth: true
                text: I18n.tr("Run commands in the terminal")
                color: Theme.colors.textMuted
            }

            PillButton {
                style: "filled"
                text: I18n.tr("Run")
                enabled: root.q.trim() !== ""
                onClicked: root.go()
            }
        }
    }
}
