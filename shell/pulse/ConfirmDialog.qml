import QtQuick
import QtQuick.Layouts
import qs
import qs.components

// "Force Firefox to quit?" and friends (PulseUi.ask).
Scrim {
    id: root

    readonly property var c: PulseUi.confirm
    property var shownC: null
    onCChanged: if (c) shownC = c

    open: c !== null
    onDismissed: PulseUi.confirm = null

    ColumnLayout {
        width: parent.width
        spacing: Theme.spacing.md

        StyledText {
            Layout.fillWidth: true
            text: root.shownC?.title ?? ""
            font.pixelSize: Theme.font.large
            font.weight: Theme.font.weightSemiBold
            wrapMode: Text.Wrap
        }

        StyledText {
            Layout.fillWidth: true
            text: root.shownC?.text ?? ""
            color: Theme.colors.textMuted
            wrapMode: Text.Wrap
        }

        RowLayout {
            Layout.fillWidth: true
            Layout.topMargin: Theme.spacing.md
            spacing: Theme.spacing.sm

            Item {
                Layout.fillWidth: true
            }

            PillButton {
                text: I18n.tr("Cancel")
                onClicked: PulseUi.confirm = null
            }

            PillButton {
                style: root.shownC?.danger ? "danger" : "filled"
                text: root.shownC?.confirmLabel ?? ""
                onClicked: {
                    const a = root.shownC?.action;
                    PulseUi.confirm = null;
                    if (a)
                        a();
                }
            }
        }
    }
}
