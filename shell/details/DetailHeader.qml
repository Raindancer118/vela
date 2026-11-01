import QtQuick
import QtQuick.Layouts
import qs
import qs.components
import qs.services

// Title row of a detail view. Children go between title and switch.
RowLayout {
    id: root

    property string title
    property bool showSwitch: false
    property bool checked: false
    property bool switchEnabled: true
    default property alias actions: actionRow.data
    // The close button closes the detail view; off for a header in a window
    // of its own, which handles `closeClicked` itself.
    property bool closesDetail: true

    signal toggled
    signal closeClicked

    spacing: Theme.spacing.sm

    IconButton {
        icon: "close"
        tonal: false
        onClicked: {
            root.closeClicked();
            if (root.closesDetail)
                ShellState.closeDetail();
        }
    }

    StyledText {
        Layout.fillWidth: true
        text: root.title
        font.pixelSize: Theme.font.large
        font.weight: Theme.font.weightMedium
    }

    Row {
        id: actionRow

        spacing: Theme.spacing.xs
    }

    Switch {
        visible: root.showSwitch
        enabled: root.switchEnabled
        checked: root.checked
        onToggled: root.toggled()
    }
}
