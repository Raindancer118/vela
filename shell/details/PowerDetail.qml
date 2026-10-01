import QtQuick
import QtQuick.Layouts
import qs
import qs.components
import qs.services
import Quickshell

// Power profiles to choose from; the current one is highlighted.
ColumnLayout {
    spacing: Theme.spacing.md

    DetailHeader {
        Layout.fillWidth: true
        title: I18n.tr("Power mode")
    }

    Item {
        Layout.fillWidth: true
        implicitHeight: rows.implicitHeight

        SlidingHighlight {
            target: {
                rep.count;
                return rep.itemAt(PowerMode.profiles.indexOf(PowerMode.profile));
            }
        }

        Column {
            id: rows

            width: parent.width
            spacing: Theme.spacing.xs

            Repeater {
                id: rep

                model: PowerMode.profiles

                ListRow {
                    id: row

                    required property string modelData

                    width: rows.width
                    ownBackground: false
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

    // Whether the computer goes to sleep on its own (vela idle / hypridle).
    ListRow {
        Layout.fillWidth: true
        icon: VelaConfig.idle.suspend ? "bedtime" : "bedtime_off"
        title: I18n.tr("Sleep automatically")
        subtitle: VelaConfig.idle.suspend ? I18n.tr("After %1 min", Math.round(VelaConfig.idle.suspendAfterMin)) : I18n.tr("Never")
        clickable: false

        Switch {
            checked: VelaConfig.idle.suspend
            onToggled: Quickshell.execDetached([Quickshell.env("VELA_BIN") || "vela", "set", "idle.suspend", String(!checked)])
        }
    }
}
