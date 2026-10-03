import QtQuick
import QtQuick.Layouts
import qs
import qs.components

// Right-click menu (PulseUi.openMenu); unfolds from the cursor.
Item {
    id: root

    readonly property var m: PulseUi.menu
    property var shownM: null
    onMChanged: if (m) shownM = m

    visible: menuCard.opacity > 0

    MouseArea {
        anchors.fill: parent
        enabled: root.m !== null
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        onPressed: PulseUi.menu = null
    }

    Rectangle {
        id: menuCard

        x: Math.min(root.shownM?.x ?? 0, root.width - width - 8)
        y: Math.min(root.shownM?.y ?? 0, root.height - height - 8)
        width: 240
        height: col.implicitHeight + 2 * Theme.spacing.xs
        radius: Theme.radius.small + 4
        color: Theme.colors.background
        border.width: 1
        border.color: Theme.colors.outline
        opacity: root.m ? 1 : 0
        transformOrigin: Item.TopLeft
        scale: root.m ? 1 : 0.9
        Behavior on opacity {
            Anim {
                duration: Theme.anim.fast
            }
        }
        Behavior on scale {
            SpringAnim {
                duration: Theme.anim.normal
            }
        }

        Column {
            id: col

            x: Theme.spacing.xs
            y: Theme.spacing.xs
            width: parent.width - 2 * Theme.spacing.xs

            Repeater {
                model: root.shownM?.items ?? []

                Item {
                    id: entry

                    required property var modelData
                    readonly property bool sep: modelData.separator === true
                    readonly property bool on: modelData.enabled !== false

                    width: col.width
                    height: sep ? 9 : 34

                    Rectangle {
                        visible: entry.sep
                        anchors.centerIn: parent
                        width: parent.width - 2 * Theme.spacing.sm
                        height: 1
                        color: Theme.colors.divider
                    }

                    Clickable {
                        visible: !entry.sep
                        enabled: entry.on
                        radius: Theme.radius.small
                        onClicked: {
                            const a = entry.modelData.action;
                            PulseUi.menu = null;
                            if (a)
                                a();
                        }
                    }

                    RowLayout {
                        visible: !entry.sep
                        anchors.fill: parent
                        anchors.leftMargin: Theme.spacing.sm
                        anchors.rightMargin: Theme.spacing.sm
                        spacing: Theme.spacing.sm
                        opacity: entry.on ? 1 : Theme.opacity.disabled

                        Item {
                            implicitWidth: Theme.icon.small
                            implicitHeight: Theme.icon.small

                            MaterialIcon {
                                visible: !entry.modelData.claude
                                anchors.centerIn: parent
                                icon: entry.modelData.icon ?? ""
                                size: Theme.icon.small
                                color: entry.modelData.danger ? Theme.pulse.crit : Theme.colors.text
                            }

                            Image {
                                visible: entry.modelData.claude === true
                                anchors.fill: parent
                                source: Qt.resolvedUrl("../assets/claude.svg")
                                sourceSize: Qt.size(32, 32)
                            }
                        }

                        StyledText {
                            Layout.fillWidth: true
                            text: entry.modelData.label ?? ""
                            color: entry.modelData.danger ? Theme.pulse.crit : Theme.colors.text
                        }

                        // The window key for it.
                        Rectangle {
                            visible: (entry.modelData.hint ?? "") !== ""
                            implicitWidth: Math.max(20, hintText.implicitWidth + 10)
                            implicitHeight: 20
                            radius: 5
                            color: Theme.colors.chip
                            border.width: 1
                            border.color: Theme.colors.outline

                            StyledText {
                                id: hintText

                                anchors.centerIn: parent
                                text: entry.modelData.hint ?? ""
                                color: Theme.colors.textMuted
                                font.pixelSize: Theme.font.small - 1
                            }
                        }
                    }
                }
            }
        }
    }
}
