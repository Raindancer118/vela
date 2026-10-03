import QtQuick
import QtQuick.Layouts
import qs
import qs.components
import qs.pulse
import "../Fmt.js" as Fmt
import "../Model.js" as Model

// What's wrong, why, and the button that fixes it.
Flickable {
    id: page

    readonly property var loc: Qt.locale()
    readonly property int problems: Pulse.findings.filter(f => f.severity !== "info").length
    readonly property int hints: Pulse.findings.length - problems

    contentHeight: col.implicitHeight + 2 * Theme.spacing.xl
    clip: true
    boundsBehavior: Flickable.StopAtBounds

    Component.onCompleted: Model.sync(findingModel, Pulse.findings.map(f => f.id))

    Connections {
        target: Pulse

        function onFrameArrived(): void {
            Model.sync(findingModel, Pulse.findings.map(f => f.id));
        }
    }

    ListModel {
        id: findingModel
    }

    ColumnLayout {
        id: col

        x: Theme.spacing.xl
        y: Theme.spacing.xl
        width: page.width - 2 * Theme.spacing.xl
        spacing: Theme.spacing.lg

        // Hero
        Tile {
            Layout.fillWidth: true
            Layout.preferredHeight: Math.max(190, heroCol.implicitHeight + 2 * Theme.spacing.lg)

            RowLayout {
                anchors.fill: parent
                spacing: Theme.spacing.xl

                Ring {
                    Layout.preferredWidth: 140
                    Layout.preferredHeight: 140
                    value: Pulse.score
                    thickness: 12
                    color: Pulse.score >= 85 ? Theme.pulse.ok : Pulse.score >= 60 ? Theme.pulse.warn : Theme.pulse.crit
                    alert: Pulse.findings.some(x => x.severity === "critical")

                    Counter {
                        anchors.centerIn: parent
                        value: Pulse.score
                        font.pixelSize: 40
                        font.weight: Theme.font.weightSemiBold
                    }
                }

                ColumnLayout {
                    id: heroCol

                    // Width from the row, not from the text (which then wraps).
                    Layout.preferredWidth: 0
                    Layout.fillWidth: true
                    spacing: Theme.spacing.xs

                    SwapText {
                        Layout.fillWidth: true
                        value: page.problems === 0 && page.hints === 0 ? I18n.tr("Your system is healthy") : page.problems === 0 ? I18n.tr("Nothing serious") : page.problems === 1 ? I18n.tr("One problem needs attention") : I18n.tr("%1 problems need attention", page.problems)
                        font.pixelSize: Theme.font.large + 4
                        font.weight: Theme.font.weightSemiBold
                    }

                    StyledText {
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        color: Theme.colors.textMuted
                        text: I18n.tr("Pulse checks load, memory, disks, heat, power, crashes, services and apps that hang or run outdated code — every second, while this window is open.") + (page.hints === 1 ? " " + I18n.tr("One hint below.") : page.hints > 1 ? " " + I18n.tr("%1 hints below.", page.hints) : "")
                    }

                    Item {
                        Layout.fillHeight: true
                    }

                    RowLayout {
                        visible: VelaConfig.pulse.claude
                        Layout.fillWidth: true
                        spacing: Theme.spacing.sm

                        // Claude gets the whole picture and digs deeper.
                        Rectangle {
                            implicitHeight: Theme.size.pillButton + 4
                            implicitWidth: claudeRow.implicitWidth + 2 * Theme.spacing.lg
                            radius: Theme.radius.small
                            color: Theme.withAlpha(Theme.colors.claude, claudeArea.containsMouse ? 0.28 : 0.18)
                            border.width: 1
                            border.color: Theme.withAlpha(Theme.colors.claude, 0.5)
                            Behavior on color {
                                ColorAnim {
                                    duration: Theme.anim.fast
                                }
                            }

                            MouseArea {
                                id: claudeArea

                                anchors.fill: parent
                                hoverEnabled: true
                                cursorShape: Qt.PointingHandCursor
                                onClicked: Pulse.askClaude("")
                            }

                            RowLayout {
                                id: claudeRow

                                anchors.centerIn: parent
                                spacing: Theme.spacing.sm

                                Image {
                                    source: Qt.resolvedUrl("../../assets/claude.svg")
                                    sourceSize: Qt.size(36, 36)
                                    Layout.preferredWidth: 18
                                    Layout.preferredHeight: 18
                                }

                                StyledText {
                                    text: I18n.tr("Ask Claude to investigate")
                                    font.weight: Theme.font.weightMedium
                                }
                            }
                        }

                        StyledText {
                            Layout.fillWidth: true
                            wrapMode: Text.Wrap
                            text: I18n.tr("Opens Claude Code with a snapshot of your system.")
                            color: Theme.colors.textMuted
                            font.pixelSize: Theme.font.small
                        }
                    }
                }
            }
        }

        // All good
        ColumnLayout {
            visible: findingModel.count === 0 && Pulse.frame !== null
            Layout.fillWidth: true
            Layout.topMargin: Theme.spacing.xl
            spacing: Theme.spacing.sm

            Rectangle {
                Layout.alignment: Qt.AlignHCenter
                implicitWidth: 72
                implicitHeight: 72
                radius: 36
                color: Theme.withAlpha(Theme.pulse.ok, 0.16)
                scale: parent.visible ? 1 : 0.5
                Behavior on scale {
                    SpringAnim {
                        duration: Theme.anim.slow * 2
                    }
                }

                MaterialIcon {
                    anchors.centerIn: parent
                    icon: "check"
                    size: 36
                    color: Theme.pulse.ok
                }
            }

            StyledText {
                Layout.alignment: Qt.AlignHCenter
                text: I18n.tr("No problems found")
                font.pixelSize: Theme.font.title
                font.weight: Theme.font.weightMedium
            }
        }

        // Findings
        ListView {
            id: findings

            Layout.fillWidth: true
            Layout.preferredHeight: contentHeight
            interactive: false
            model: findingModel
            spacing: Theme.spacing.sm

            add: Transition {
                ParallelAnimation {
                    Anim {
                        property: "opacity"
                        from: 0
                        to: 1
                        duration: Theme.anim.slow
                    }
                    SpringAnim {
                        property: "scale"
                        from: 0.96
                        to: 1
                    }
                }
            }
            remove: Transition {
                ParallelAnimation {
                    Anim {
                        property: "opacity"
                        to: 0
                    }
                    Anim {
                        property: "x"
                        to: 60
                    }
                }
            }
            displaced: Transition {
                SpringAnim {
                    property: "y"
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
                id: card

                required property string key
                readonly property var fd: Pulse.findings.find(x => x.id === key) ?? null
                readonly property color accent: Words.severityColor(fd?.severity ?? "info")

                width: ListView.view.width
                height: cardRow.implicitHeight + 2 * Theme.spacing.lg
                radius: Theme.radius.card
                color: Theme.colors.tile
                border.width: 1
                border.color: Theme.withAlpha(accent, 0.35)

                Rectangle {
                    x: 0
                    y: Theme.spacing.md
                    width: 4
                    height: parent.height - 2 * Theme.spacing.md
                    radius: 2
                    color: card.accent
                }

                RowLayout {
                    id: cardRow

                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.top: parent.top
                    anchors.margins: Theme.spacing.lg
                    anchors.leftMargin: Theme.spacing.xl
                    spacing: Theme.spacing.lg

                    Rectangle {
                        Layout.alignment: Qt.AlignTop
                        implicitWidth: 40
                        implicitHeight: 40
                        radius: 20
                        color: Theme.withAlpha(card.accent, 0.15)

                        MaterialIcon {
                            anchors.centerIn: parent
                            icon: Words.severityIcon(card.fd?.severity ?? "info")
                            color: card.accent
                        }
                    }

                    ColumnLayout {
                        id: cardCol

                        Layout.fillWidth: true
                        spacing: Theme.spacing.xs

                        StyledText {
                            Layout.fillWidth: true
                            text: card.fd ? Words.title(card.fd) : ""
                            font.pixelSize: Theme.font.title
                            font.weight: Theme.font.weightSemiBold
                            wrapMode: Text.Wrap
                        }

                        StyledText {
                            Layout.fillWidth: true
                            text: card.fd ? Words.text(card.fd) : ""
                            color: Theme.colors.textMuted
                            wrapMode: Text.Wrap
                        }

                        // The apps involved.
                        Flow {
                            Layout.fillWidth: true
                            Layout.topMargin: 2
                            spacing: Theme.spacing.xs
                            visible: (card.fd?.apps ?? []).length > 0

                            Repeater {
                                model: (card.fd?.apps ?? []).slice(0, 8)

                                Rectangle {
                                    required property string modelData
                                    readonly property var a: Pulse.app(modelData)

                                    visible: a !== null
                                    implicitHeight: 28
                                    implicitWidth: chipRow.implicitWidth + 2 * Theme.spacing.sm
                                    radius: 14
                                    color: Theme.colors.chip

                                    Clickable {
                                        radius: 14
                                        onClicked: PulseUi.showApp(parent.modelData)
                                    }

                                    RowLayout {
                                        id: chipRow

                                        anchors.centerIn: parent
                                        spacing: Theme.spacing.xs

                                        AppIcon {
                                            size: 16
                                            icon: parent.parent.a?.icon ?? ""
                                            name: parent.parent.a?.name ?? ""
                                        }

                                        StyledText {
                                            text: parent.parent.a?.name ?? ""
                                            font.pixelSize: Theme.font.small
                                        }
                                    }
                                }
                            }
                        }

                        Flow {
                            Layout.fillWidth: true
                            Layout.topMargin: Theme.spacing.xs
                            spacing: Theme.spacing.xs
                            visible: (card.fd?.fixes ?? []).length > 0

                            Repeater {
                                model: (card.fd?.fixes ?? []).filter(x => x.action !== "claude" || VelaConfig.pulse.claude)

                                PillButton {
                                    required property var modelData
                                    required property int index

                                    style: modelData.action === "force" ? "danger" : index === 0 && modelData.action !== "show-app" && modelData.action !== "show-perf" ? "filled" : "tonal"
                                    icon: Words.fixIcon(modelData.action)
                                    text: Words.fix(modelData)
                                    onClicked: PulseUi.doFix(modelData)
                                }
                            }
                        }
                    }
                }
            }
        }

        // Crash history
        ColumnLayout {
            visible: Pulse.crashes.length > 0
            Layout.fillWidth: true
            spacing: Theme.spacing.xs

            SectionLabel {
                text: I18n.tr("Crashes in the last 24 hours")
            }

            Repeater {
                model: Pulse.crashes

                Rectangle {
                    required property var modelData

                    Layout.fillWidth: true
                    implicitHeight: 52
                    radius: Theme.radius.small
                    color: Theme.colors.tile

                    RowLayout {
                        anchors.fill: parent
                        anchors.leftMargin: Theme.spacing.md
                        anchors.rightMargin: Theme.spacing.sm
                        spacing: Theme.spacing.md

                        AppIcon {
                            size: 26
                            icon: parent.parent.modelData.icon
                            name: parent.parent.modelData.name
                        }

                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 0

                            StyledText {
                                text: parent.parent.parent.modelData.name
                                font.weight: Theme.font.weightMedium
                            }

                            StyledText {
                                Layout.fillWidth: true
                                text: parent.parent.parent.modelData.exe
                                color: Theme.colors.textMuted
                                font.pixelSize: Theme.font.small
                            }
                        }

                        Chip {
                            text: parent.parent.modelData.count === 1 ? I18n.tr("once") : I18n.tr("%1 times", parent.parent.modelData.count)
                            tint: parent.parent.modelData.count >= 3 ? Theme.pulse.crit : Theme.pulse.warn
                        }

                        Chip {
                            text: parent.parent.modelData.signal
                        }

                        StyledText {
                            Layout.preferredWidth: 90
                            horizontalAlignment: Text.AlignRight
                            text: Words.ago(parent.parent.modelData.last / 1000)
                            color: Theme.colors.textMuted
                            font.pixelSize: Theme.font.small
                        }

                        PillButton {
                            visible: VelaConfig.pulse.claude
                            text: I18n.tr("Ask Claude")
                            onClicked: Pulse.askClaude("crash:" + parent.parent.modelData.exe)
                        }
                    }
                }
            }
        }

        Item {
            Layout.preferredHeight: Theme.spacing.xl
        }
    }
}
