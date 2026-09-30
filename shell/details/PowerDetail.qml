import QtQuick
import QtQuick.Layouts
import qs
import qs.components
import qs.services

// Power profiles to choose from; the current one is highlighted.
ColumnLayout {
    spacing: Theme.spacing.md

    DetailHeader {
        Layout.fillWidth: true
        title: I18n.tr("Power mode")
    }

    ColumnLayout {
        Layout.fillWidth: true
        spacing: Theme.spacing.xs

        Repeater {
            model: PowerMode.profiles

            ListRow {
                id: row

                required property string modelData

                Layout.fillWidth: true
                icon: PowerMode.icon(modelData)
                title: PowerMode.label(modelData)
                subtitle: PowerMode.description(modelData)
                highlighted: PowerMode.profile === modelData
                clickable: !highlighted
                onClicked: PowerMode.set(modelData)

                MaterialIcon {
                    visible: row.highlighted
                    icon: "check"
                    size: Theme.icon.small
                    color: Theme.colors.primary
                }
            }
        }
    }
}
