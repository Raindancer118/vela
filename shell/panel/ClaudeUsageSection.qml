import QtQuick
import QtQuick.Layouts
import qs
import qs.components
import qs.services

// Claude plan usage per profile: 5-hour and 7-day window, coloured by how
// much is used, with the reset time. Card by default; the subtle style is
// grey text under a thin line at the bottom edge.
Card {
    id: root

    readonly property var accounts: ClaudeUsage.shown
    readonly property bool subtle: Config.claudeUsageSubtle
    readonly property real pad: subtle ? 0 : Theme.spacing.md

    visible: Config.claudeUsage && accounts.length > 0
    implicitHeight: column.implicitHeight + 2 * pad + (subtle ? Theme.spacing.sm : 0)
    color: subtle ? "transparent" : Theme.colors.surface

    Behavior on color {
        ColorAnim {}
    }

    // Subtle style: the line separating it from the notifications.
    Rectangle {
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        height: Theme.size.border
        color: Theme.colors.outline
        opacity: root.subtle ? 1 : 0

        Behavior on opacity {
            Anim {}
        }
    }

    // Plan as quiet small caps ("PRO": the first letter a touch larger).
    component PlanBadge: StyledText {
        property string plan

        visible: plan !== ""
        textFormat: Text.RichText
        text: {
            const p = plan.toUpperCase();
            return p.charAt(0) + "<span style='font-size:" + Math.round(font.pixelSize * 0.82) + "px'>" + p.slice(1) + "</span>";
        }
        color: Theme.withAlpha(Theme.colors.text, 0.5)
        font.pixelSize: Theme.font.small
        font.weight: Theme.font.weightSemiBold
        font.letterSpacing: Theme.font.labelLetterSpacing
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
                opacity: root.subtle ? 0.55 : 1

                Behavior on opacity {
                    Anim {}
                }

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
            color: root.subtle ? Theme.colors.textMuted : row.tone
            font.pixelSize: Theme.font.small
            font.weight: Theme.font.weightMedium
        }

        // Hidden, not removed: every bar keeps the same length.
        MaterialIcon {
            opacity: reset.text !== "" ? 1 : 0
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
        anchors.margins: root.pad
        anchors.topMargin: root.pad + (root.subtle ? Theme.spacing.sm : 0)
        spacing: root.subtle ? Theme.spacing.xs : Theme.spacing.sm

        RowLayout {
            Layout.fillWidth: true
            visible: !root.subtle
            spacing: Theme.spacing.sm

            Image {
                source: Qt.resolvedUrl("../assets/claude.svg")
                sourceSize.width: Theme.icon.small
                sourceSize.height: Theme.icon.small
            }

            StyledText {
                Layout.alignment: Qt.AlignBaseline
                text: "Claude"
                font.weight: Theme.font.weightMedium
            }

            // One account: its plan next to the title.
            PlanBadge {
                Layout.alignment: Qt.AlignBaseline
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
                        Layout.alignment: Qt.AlignBaseline
                        text: account.modelData.name === "default" ? I18n.tr("Default") : account.modelData.name
                        color: Theme.colors.textMuted
                        font.pixelSize: Theme.font.small
                        font.weight: Theme.font.weightMedium
                    }

                    PlanBadge {
                        Layout.alignment: Qt.AlignBaseline
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
