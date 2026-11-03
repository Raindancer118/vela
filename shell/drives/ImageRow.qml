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
    // Part of a choice of several; `several`: such a choice is going on.
    property bool checked: false
    property bool several: false
    property bool fits: true
    signal picked
    signal toggled
    signal keep

    function esc(s: string): string {
        return s.replace(/&/g, "&amp;").replace(/</g, "&lt;");
    }

    implicitHeight: 60

    // Ticked rows of a choice of several; the single choice has the gliding highlight.
    Rectangle {
        anchors.fill: parent
        radius: Theme.radius.small
        color: Theme.colors.selected
        border.width: Theme.size.border
        border.color: Theme.colors.selectedRing
        opacity: root.several && root.checked ? 1 : 0
        Behavior on opacity {
            Anim {
                duration: Theme.anim.fast
            }
        }
    }

    Clickable {
        id: area

        radius: Theme.radius.small
        onClicked: mouse => (mouse.modifiers & (Qt.ControlModifier | Qt.ShiftModifier)) ? root.toggled() : root.picked()
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

        // Tick to pick several (they go onto the stick with Ventoy).
        Rectangle {
            implicitWidth: 22
            implicitHeight: 22
            radius: 11
            color: root.checked && root.several ? Theme.colors.primary : "transparent"
            border.width: root.checked && root.several ? 0 : 1.5
            border.color: tickArea.containsMouse ? Theme.colors.text : Theme.colors.textMuted
            opacity: root.several || area.containsMouse || tickArea.containsMouse ? 1 : 0
            Behavior on opacity {
                Anim {
                    duration: Theme.anim.fast
                }
            }
            Behavior on color {
                ColorAnim {
                    duration: Theme.anim.fast
                }
            }

            MaterialIcon {
                anchors.centerIn: parent
                visible: root.checked && root.several
                icon: "check"
                size: 16
                color: Theme.colors.textOnPrimary
            }

            MouseArea {
                id: tickArea

                anchors.fill: parent
                anchors.margins: -6
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: root.toggled()
            }

            Tip {
                text: root.several && root.checked ? I18n.tr("Leave out") : I18n.tr("Pick several (Ctrl-click): they go onto the stick with Ventoy")
                shown: tickArea.containsMouse
            }
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
