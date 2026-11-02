import QtQuick
import QtQuick.Layouts
import qs
import qs.components
import qs.pulse
import "Words.js" as W

// One image of the library: a coloured monogram for the system, its name
// and version, and what to know before writing it.
Item {
    id: root

    required property var image
    property bool selected: false
    property bool fits: true
    signal picked
    signal keep

    function esc(s: string): string {
        return s.replace(/&/g, "&amp;").replace(/</g, "&lt;");
    }

    implicitHeight: 60

    Clickable {
        radius: Theme.radius.small
        onClicked: root.picked()
    }

    RowLayout {
        anchors.fill: parent
        anchors.leftMargin: Theme.spacing.md
        anchors.rightMargin: Theme.spacing.md
        spacing: Theme.spacing.md
        opacity: root.fits ? 1 : 0.55

        Monogram {
            system: root.image.system
            family: root.image.family
        }

        ColumnLayout {
            Layout.fillWidth: true
            spacing: 2

            StyledText {
                Layout.fillWidth: true
                textFormat: Text.StyledText
                text: root.esc(root.image.system) + (root.image.version ? `&nbsp;&nbsp;<font color="${Theme.colors.textMuted}">${root.esc(root.image.version)}</font>` : "")
                font.weight: Theme.font.weightMedium
                elide: Text.ElideRight
            }

            StyledText {
                Layout.fillWidth: true
                text: [root.image.arch, W.size(root.image.size, Qt.locale()), root.image.file].filter(s => s).join("  ·  ")
                color: Theme.colors.textMuted
                font.pixelSize: Theme.font.small
                elide: Text.ElideMiddle
            }
        }

        Chip {
            visible: !!root.image.compression
            text: (root.image.compression ?? "").toUpperCase()
        }

        Chip {
            visible: root.image.windows
            icon: "warning"
            text: I18n.tr("Won't boot this way")
            tint: Theme.pulse.warn

            Tip {
                text: I18n.tr("Windows installers need a special stick (e.g. Ventoy or WoeUSB); copied byte for byte they don't start.")
                shown: hover.hovered
            }

            HoverHandler {
                id: hover
            }
        }

        Chip {
            visible: !root.fits
            text: I18n.tr("Too big")
            tint: Theme.pulse.crit
        }

        PillButton {
            visible: root.image.extra
            text: I18n.tr("Keep")
            icon: "download"
            onClicked: root.keep()

            Tip {
                text: I18n.tr("Copy it into the image folder")
                shown: parent.hovered
            }
        }
    }
}
