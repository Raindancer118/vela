import QtQuick
import QtQuick.Layouts
import qs
import qs.components

// Page title with a live subtitle, the search field and "Run new task".
Item {
    id: root

    readonly property bool searchable: ["processes", "services", "activity"].indexOf(PulseUi.page) >= 0

    function focusSearch(): void {
        if (!searchable)
            PulseUi.show("processes");
        search.input.forceActiveFocus();
    }

    implicitHeight: 72

    RowLayout {
        anchors.fill: parent
        anchors.leftMargin: Theme.spacing.xl
        anchors.rightMargin: Theme.spacing.lg
        spacing: Theme.spacing.md

        ColumnLayout {
            Layout.fillWidth: true
            spacing: 0

            SwapText {
                Layout.fillWidth: true
                value: ({
                        overview: I18n.tr("Overview"),
                        processes: I18n.tr("Apps & processes"),
                        performance: I18n.tr("Performance"),
                        diagnosis: I18n.tr("Diagnosis"),
                        services: I18n.tr("Services"),
                        activity: I18n.tr("Activity")
                    })[PulseUi.page] ?? ""
                font.pixelSize: Theme.font.large + 2
                font.weight: Theme.font.weightSemiBold
            }

            StyledText {
                Layout.fillWidth: true
                color: Theme.colors.textMuted
                font.pixelSize: Theme.font.small
                text: {
                    const f = Pulse.frame;
                    if (!f)
                        return "";
                    const windows = Pulse.apps.filter(a => a.kind === "window").length;
                    const bg = Pulse.apps.filter(a => a.kind === "background").length;
                    const parts = [I18n.tr("%1 apps", windows), I18n.tr("%1 in the background", bg), I18n.tr("%1 processes", f.cpu.processes), I18n.uptime(f.cpu.uptime)];
                    return parts.join("  ·  ");
                }
            }
        }

        TextField {
            id: search

            visible: root.searchable
            Layout.preferredWidth: 280
            placeholder: I18n.tr("Search  (Ctrl+F)")
            text: PulseUi.query
            onTextChanged: PulseUi.query = text
            input.Keys.onEscapePressed: {
                PulseUi.query = "";
                search.input.focus = false;
            }

            Connections {
                target: PulseUi

                function onQueryChanged(): void {
                    if (search.text !== PulseUi.query)
                        search.text = PulseUi.query;
                }
            }
        }

        PillButton {
            icon: "add"
            text: I18n.tr("Run new task")
            onClicked: PulseUi.runOpen = true
        }
    }

    Rectangle {
        anchors.bottom: parent.bottom
        width: parent.width
        height: 1
        color: Theme.colors.divider
    }
}
