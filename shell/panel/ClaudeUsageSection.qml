import QtQuick
import QtQuick.Layouts
import qs
import qs.components
import qs.services

// Claude plan usage per profile: 5-hour and 7-day window, coloured by how
// much is used, with the reset time.
Card {
    id: root

    readonly property var accounts: ClaudeUsage.accounts

    visible: Config.claudeUsage && accounts.length > 0
    implicitHeight: column.implicitHeight + 2 * Theme.spacing.md

    // Plan name as a small accent pill ("Pro", "Max 20×", "Team").
    component PlanBadge: Rectangle {
        property string plan

        visible: plan !== ""
        implicitWidth: planText.implicitWidth + 2 * Theme.spacing.sm
        implicitHeight: planText.implicitHeight + Theme.spacing.xs
        radius: height / 2
        color: Theme.withAlpha(Theme.colors.primary, 0.18)

        StyledText {
            id: planText

            anchors.centerIn: parent
            text: parent.plan
            color: Theme.colors.primary
            font.pixelSize: Theme.font.small
            font.weight: Theme.font.weightSemiBold
        }
    }

    component UsageRow: RowLayout {
        id: row

        property string label
        property var window: null
        // A window whose reset has passed is empty again.
        readonly property real value: !window ? 0 : window.resetsAt > 0 && window.resetsAt <= ClaudeUsage.now ? 0 : window.utilization
        readonly property color tone: {
            switch (ClaudeUsage.level(value)) {
            case "high":
                return Theme.colors.usageHigh;
            case "mid":
                return Theme.colors.usageMid;
            default:
                return Theme.colors.usageLow;
            }
        }

        visible: window !== null
        spacing: Theme.spacing.sm

        StyledText {
            Layout.preferredWidth: Theme.size.usageLabelWidth
            text: row.label
            color: Theme.colors.textMuted
            font.pixelSize: Theme.font.small
        }

        Rectangle {
            Layout.fillWidth: true
            implicitHeight: Theme.size.usageBarHeight
            radius: height / 2
            color: Theme.colors.surfaceHighest

            Rectangle {
                height: parent.height
                width: Math.max(height, parent.width * Math.min(1, row.value))
                radius: height / 2
                color: row.tone

                Behavior on width {
                    Anim {}
                }

                Behavior on color {
                    ColorAnim {}
                }
            }
        }

        StyledText {
            Layout.preferredWidth: Theme.size.usagePercentWidth
            horizontalAlignment: Text.AlignRight
            text: Math.round(row.value * 100) + " %"
            color: row.tone
            font.pixelSize: Theme.font.small
            font.weight: Theme.font.weightMedium
        }

        MaterialIcon {
            visible: reset.text !== ""
            icon: "restart_alt"
            size: Theme.icon.small * 0.8
            color: Theme.colors.textMuted
        }

        StyledText {
            id: reset

            Layout.preferredWidth: Theme.size.usageResetWidth
            text: ClaudeUsage.resetLabel(row.window?.resetsAt ?? 0, ClaudeUsage.now)
            color: Theme.colors.textMuted
            font.pixelSize: Theme.font.small
        }
    }

    ColumnLayout {
        id: column

        anchors.fill: parent
        anchors.margins: Theme.spacing.md
        spacing: Theme.spacing.sm

        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.spacing.sm

            Image {
                source: Qt.resolvedUrl("../assets/claude.svg")
                sourceSize.width: Theme.icon.small
                sourceSize.height: Theme.icon.small
            }

            StyledText {
                text: "Claude"
                font.weight: Theme.font.weightMedium
            }

            // One account: its plan next to the title.
            PlanBadge {
                plan: root.accounts.length === 1 ? (root.accounts[0].plan ?? "") : ""
            }

            Item {
                Layout.fillWidth: true
            }
        }

        Repeater {
            model: root.accounts

            ColumnLayout {
                id: account

                required property var modelData

                Layout.fillWidth: true
                spacing: Theme.spacing.xs

                RowLayout {
                    visible: root.accounts.length > 1
                    spacing: Theme.spacing.sm

                    StyledText {
                        text: account.modelData.name === "default" ? I18n.tr("Default") : account.modelData.name
                        color: Theme.colors.textMuted
                        font.pixelSize: Theme.font.small
                        font.weight: Theme.font.weightMedium
                    }

                    PlanBadge {
                        plan: account.modelData.plan ?? ""
                    }
                }

                UsageRow {
                    Layout.fillWidth: true
                    label: "5 h"
                    window: account.modelData.session
                }

                UsageRow {
                    Layout.fillWidth: true
                    label: I18n.tr("7 d")
                    window: account.modelData.week
                }
            }
        }
    }
}
