import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import qs
import qs.components
import qs.services
import qs.quickshare

// Receive tab of the Quick Share view: this PC is visible while Quick Share
// is on; here it gets its name. Offers themselves open their own window.
ColumnLayout {
    id: root

    readonly property int circle: Theme.icon.huge + 2 * Theme.spacing.lg
    // The name phones see when none is set (like the backend: the hostname).
    readonly property string systemName: hostname.text().trim() || "Linux"
    // 0..1, one wave of the pulse around the circle.
    property real wave: 0
    // In Quick Share's window: the big hero, what can reach this PC and what
    // came in lately.
    property bool roomy: false

    // The newest transfer that is running, either way, for the hero.
    readonly property var running: {
        let found = null;
        for (const id in QuickShare.transfers) {
            const t = QuickShare.transfers[id];
            if (t.state === "transferring" && (!found || Number(id) > found.id))
                found = t;
        }
        return found;
    }
    // What came in lately, newest first.
    readonly property var recent: Object.keys(QuickShare.transfers).map(Number).sort((a, b) => b - a).map(id => QuickShare.transfers[id]).filter(t => t.direction === "in" && t.state === "done" && (t.saved ?? []).length > 0).slice(0, 3)

    function baseName(path: string): string {
        return path.split("/").pop();
    }

    objectName: "receive"
    spacing: Theme.spacing.sm

    NumberAnimation on wave {
        from: 0
        to: 1
        duration: Theme.anim.pulse
        loops: Animation.Infinite
        running: root.visible && Theme.anim.pulse > 0
    }

    FileView {
        id: hostname

        path: "/proc/sys/kernel/hostname"
        blockLoading: true
        printErrors: false
    }

    QuickShareHero {
        objectName: "hero"
        visible: root.roomy
        Layout.fillWidth: true
        Layout.topMargin: Theme.spacing.sm
        Layout.preferredHeight: 230
        progress: root.running && root.running.total > 0 ? root.running.done / root.running.total : -1
        incoming: root.running?.direction !== "out"
        peerName: root.running?.device ?? ""
    }

    Item {
        Layout.fillWidth: true
        Layout.topMargin: Theme.spacing.sm
        visible: !root.roomy
        implicitHeight: root.circle + 2 * Theme.spacing.xl

        // Two rings, half a wave apart, grow out of the circle and fade.
        Repeater {
            model: 2

            Rectangle {
                required property int index
                readonly property real phase: (root.wave + index / 2) % 1

                anchors.centerIn: parent
                width: root.circle
                height: width
                radius: width / 2
                color: Theme.colors.accentChip
                scale: 1 + phase * 2 * Theme.spacing.xl / root.circle
                opacity: 1 - phase
            }
        }

        Rectangle {
            anchors.centerIn: parent
            width: root.circle
            height: width
            radius: width / 2
            color: Theme.colors.accentChip

            MaterialIcon {
                anchors.centerIn: parent
                icon: "laptop"
                size: Theme.icon.huge
                color: Theme.colors.primary
            }
        }
    }

    StyledText {
        Layout.fillWidth: true
        horizontalAlignment: Text.AlignHCenter
        text: !root.roomy || !root.running ? I18n.tr("Visible to nearby devices") : root.running.direction === "out" ? I18n.tr("Sending to %1", root.running.device) : I18n.tr("Receiving from %1", root.running.device)
        font.pixelSize: Theme.font.title
        font.weight: Theme.font.weightMedium
    }

    StyledText {
        Layout.fillWidth: true
        visible: !(root.roomy && root.running)
        horizontalAlignment: Text.AlignHCenter
        text: I18n.tr("On the phone, choose Quick Share and then “%1”", QuickShare.name || root.systemName)
        color: Theme.colors.textMuted
        wrapMode: Text.WordWrap
    }

    // What a phone can reach this PC over right now.
    Row {
        objectName: "reach"
        visible: root.roomy
        Layout.alignment: Qt.AlignHCenter
        Layout.topMargin: Theme.spacing.sm
        spacing: Theme.spacing.sm

        Repeater {
            model: [
                {
                    icon: "wifi",
                    text: I18n.tr("Wi-Fi"),
                    on: QuickShare.connected
                },
                {
                    icon: "bluetooth",
                    text: I18n.tr("Bluetooth"),
                    on: QuickShare.bleReceive
                },
                {
                    icon: "signal_wifi_4_bar",
                    text: I18n.tr("Access point"),
                    on: QuickShare.hotspot
                }
            ]

            Rectangle {
                required property var modelData

                implicitWidth: chipRow.implicitWidth + 2 * Theme.spacing.md
                implicitHeight: 30
                radius: height / 2
                color: modelData.on ? Theme.colors.accentChip : Theme.colors.chip
                opacity: modelData.on ? 1 : 0.55

                Row {
                    id: chipRow

                    anchors.centerIn: parent
                    spacing: Theme.spacing.xs

                    MaterialIcon {
                        anchors.verticalCenter: parent.verticalCenter
                        icon: parent.parent.modelData.icon
                        size: 16
                        color: parent.parent.modelData.on ? Theme.colors.primary : Theme.colors.textMuted
                    }

                    StyledText {
                        anchors.verticalCenter: parent.verticalCenter
                        text: parent.parent.modelData.text
                        font.pixelSize: Theme.font.small
                        color: parent.parent.modelData.on ? Theme.colors.text : Theme.colors.textMuted
                    }
                }
            }
        }
    }

    SectionHeader {
        visible: root.roomy && root.recent.length > 0
        topPadding: Theme.spacing.md
        text: I18n.tr("Recently received")
    }

    Repeater {
        model: root.roomy ? root.recent : []

        Rectangle {
            required property var modelData

            Layout.fillWidth: true
            implicitHeight: 52
            radius: Theme.radius.md
            color: Theme.colors.surface

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: Theme.spacing.md
                anchors.rightMargin: Theme.spacing.xs
                spacing: Theme.spacing.md

                MaterialIcon {
                    icon: QuickShare.fileIcon(root.baseName(parent.parent.modelData.saved[0]), "")
                    size: 20
                    color: Theme.colors.primary
                }

                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 0

                    StyledText {
                        Layout.fillWidth: true
                        elide: Text.ElideMiddle
                        text: {
                            const saved = parent.parent.parent.modelData.saved;
                            return saved.length === 1 ? root.baseName(saved[0]) : I18n.tr("%1 files", saved.length);
                        }
                    }

                    StyledText {
                        Layout.fillWidth: true
                        elide: Text.ElideRight
                        text: parent.parent.parent.modelData.device
                        color: Theme.colors.textMuted
                        font.pixelSize: Theme.font.small
                    }
                }

                IconButton {
                    icon: "folder"
                    tonal: false
                    onClicked: Quickshell.execDetached(["xdg-open", parent.parent.modelData.dir])
                }
            }
        }
    }

    SectionHeader {
        topPadding: Theme.spacing.md
        text: I18n.tr("Name")
    }

    TextField {
        Layout.fillWidth: true
        // The config hands over the name in use: the hostname means none is set.
        text: Config.quickShareName !== root.systemName ? Config.quickShareName : ""
        placeholder: root.systemName
        onAccepted: {
            QuickShare.setName(text.trim());
            input.focus = false;
        }
    }

    StyledText {
        Layout.fillWidth: true
        text: I18n.tr("Leave empty for the system name.")
        color: Theme.colors.textMuted
        font.pixelSize: Theme.font.small
        wrapMode: Text.WordWrap
    }

    StyledText {
        Layout.fillWidth: true
        Layout.topMargin: Theme.spacing.md
        objectName: "networkHint"
        text: QuickShare.hotspot ? I18n.tr("No shared Wi-Fi needed: the phone switches to an access point of this PC.") : I18n.tr("Phone and PC must be on the same Wi-Fi.")
        color: Theme.colors.textMuted
        font.pixelSize: Theme.font.small
        wrapMode: Text.WordWrap
    }

    // Always shown: receiving with "Share with Apple devices" on is untested.
    StyledText {
        objectName: "appleHint"
        Layout.fillWidth: true
        text: I18n.tr("Phone doesn't find this PC? Turn off “Share with Apple devices” in Quick Share on the phone.")
        color: Theme.colors.textMuted
        font.pixelSize: Theme.font.small
        wrapMode: Text.WordWrap
    }
}
