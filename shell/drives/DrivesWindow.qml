import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Hyprland
import qs
import qs.components
import qs.pulse
import "Words.js" as W

// The Drives window: sticks on the left; on the right either the image
// library (write one, or copy it onto a Ventoy stick) or formatting.
FloatingWindow {
    id: win

    title: I18n.tr("Drives — Sticks & images")
    implicitWidth: 920
    implicitHeight: 640
    minimumSize: Qt.size(760, 520)
    color: "transparent"
    onVisibleChanged: if (!visible)
        Qt.quit()

    // Hyprland's own toplevel of this window. Its title arrives after the
    // window is mapped, so window rules on the title never match it and
    // "title:" selectors don't find it: go by address.
    readonly property string address: {
        const t = Hyprland.toplevels.values.find(t => t.title === win.title);
        return t ? "0x" + t.address.replace(/^0x/, "") : "";
    }
    property bool placed: false
    onAddressChanged: place()

    function place(): void {
        if (placed || address === "")
            return;
        placed = true;
        const w = `window = "address:${address}"`;
        Hyprland.dispatch(`hl.dsp.window.float({ action = "enable", ${w} })`);
        Hyprland.dispatch(`hl.dsp.window.resize({ x = ${implicitWidth}, y = ${implicitHeight}, ${w} })`);
        Hyprland.dispatch(`hl.dsp.window.center({ ${w} })`);
    }

    function raise(): void {
        if (address !== "")
            Hyprland.dispatch(`hl.dsp.focus({ window = "address:${address}" })`);
    }

    Timer {
        interval: 150
        repeat: true
        running: !win.placed
        onTriggered: Hyprland.refreshToplevels()
    }

    readonly property var drive: Drives.drive
    readonly property bool ventoyStick: !!drive?.ventoy
    // { title, text, label, action }
    property var confirm: null

    property string fs: "fat32"
    property bool erase: false
    property bool ventoy: false
    property bool verify: Drives.hello.verify ?? true

    function askWrite(): void {
        const img = Drives.image;
        if (!drive || !img)
            return;
        if (ventoyStick) {
            Drives.write(false);
            return;
        }
        confirm = {
            title: I18n.tr("Write %1 to “%2”?", img.system + (img.version ? " " + img.version : ""), DrivesText.name(drive)),
            text: I18n.tr("Everything on %1 (%2) is erased. This can't be undone.", drive.device, W.nominal(drive.size, Qt.locale())),
            label: I18n.tr("Erase and write"),
            action: () => Drives.write(win.verify)
        };
    }

    function askFormat(): void {
        if (!drive)
            return;
        confirm = {
            title: ventoy ? I18n.tr("Install Ventoy on “%1”?", DrivesText.name(drive)) : I18n.tr("Format “%1”?", DrivesText.name(drive)),
            text: I18n.tr("Everything on %1 (%2) is erased. This can't be undone.", drive.device, W.nominal(drive.size, Qt.locale())),
            label: ventoy ? I18n.tr("Erase and install") : I18n.tr("Erase and format"),
            action: () => Drives.format(win.fs, label.text, win.erase && !win.ventoy, win.ventoy)
        };
    }

    Rectangle {
        anchors.fill: parent
        color: Theme.colors.panel
    }

    Item {
        id: body

        anchors.fill: parent
        focus: true

        Keys.onPressed: event => {
            if (event.key === Qt.Key_Escape) {
                if (win.confirm)
                    win.confirm = null;
                else if (Drives.job)
                    Drives.dismissJob();
                else
                    Qt.quit();
                event.accepted = true;
            } else if (event.modifiers & Qt.ControlModifier && event.key === Qt.Key_W) {
                Qt.quit();
            }
        }

        // Image files dropped on the window are offered right away.
        DropArea {
            id: drop

            anchors.fill: parent
            keys: ["text/uri-list"]
            onDropped: d => {
                const paths = d.urls.map(u => decodeURIComponent(String(u).replace(/^file:\/\//, "")));
                Drives.open({
                    tab: "image",
                    images: paths
                });
            }
        }

        RowLayout {
            anchors.fill: parent
            spacing: 0

            // Sidebar: the drives.
            Item {
                Layout.fillHeight: true
                Layout.preferredWidth: 270

                ColumnLayout {
                    anchors.fill: parent
                    anchors.margins: Theme.spacing.md
                    spacing: Theme.spacing.xs

                    RowLayout {
                        Layout.fillWidth: true
                        Layout.leftMargin: Theme.spacing.sm
                        Layout.topMargin: Theme.spacing.sm
                        Layout.bottomMargin: Theme.spacing.lg
                        spacing: Theme.spacing.sm

                        Rectangle {
                            implicitWidth: 34
                            implicitHeight: 34
                            radius: 11
                            color: Theme.colors.accentChip

                            MaterialIcon {
                                anchors.centerIn: parent
                                icon: "usb_drive"
                                size: 20
                                color: Theme.colors.primary
                            }
                        }

                        ColumnLayout {
                            spacing: 0

                            StyledText {
                                text: I18n.tr("Drives")
                                font.pixelSize: Theme.font.large
                                font.weight: Theme.font.weightSemiBold
                            }

                            StyledText {
                                text: I18n.tr("Sticks, cards and images")
                                color: Theme.colors.textMuted
                                font.pixelSize: Theme.font.small
                            }
                        }
                    }

                    SectionLabel {
                        Layout.leftMargin: Theme.spacing.sm
                        text: I18n.tr("Drives")
                    }

                    Item {
                        Layout.fillWidth: true
                        Layout.preferredHeight: driveColumn.implicitHeight

                        SlidingHighlight {
                            target: driveRepeater.itemAt(Drives.drives.findIndex(d => d.id === Drives.driveId))
                            color: Theme.colors.selected
                            border.width: Theme.size.border
                            border.color: Theme.colors.selectedRing
                        }

                        Column {
                            id: driveColumn

                            width: parent.width
                            spacing: 2

                            Repeater {
                                id: driveRepeater

                                model: Drives.drives

                                DriveCard {
                                    required property var modelData

                                    width: driveColumn.width
                                    drive: modelData
                                    selected: modelData.id === Drives.driveId
                                    onPicked: if (!Drives.busy) Drives.driveId = modelData.id
                                }
                            }
                        }
                    }

                    StyledText {
                        visible: Drives.connected && Drives.drives.length === 0
                        Layout.fillWidth: true
                        Layout.margins: Theme.spacing.sm
                        text: I18n.tr("Plug in a USB stick or an SD card.")
                        color: Theme.colors.textMuted
                        wrapMode: Text.Wrap
                    }

                    Item {
                        Layout.fillHeight: true
                    }

                    // The image folder.
                    Item {
                        Layout.fillWidth: true
                        Layout.preferredHeight: 44

                        Clickable {
                            radius: Theme.radius.small
                            onClicked: Quickshell.execDetached(["sh", "-c", 'mkdir -p "$1" && exec xdg-open "$1"', "sh", Drives.hello.imageDir ?? ""])
                        }

                        RowLayout {
                            anchors.fill: parent
                            anchors.leftMargin: Theme.spacing.md
                            anchors.rightMargin: Theme.spacing.md
                            spacing: Theme.spacing.md

                            MaterialIcon {
                                icon: "folder"
                                color: Theme.colors.textMuted
                            }

                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: 0

                                StyledText {
                                    text: I18n.tr("Image folder")
                                    font.pixelSize: Theme.font.small
                                    color: Theme.colors.textMuted
                                }

                                StyledText {
                                    Layout.fillWidth: true
                                    text: Drives.hello.imageDirShown ?? ""
                                    elide: Text.ElideMiddle
                                }
                            }

                            MaterialIcon {
                                icon: "open_in_new"
                                size: Theme.icon.small
                                color: Theme.colors.textMuted
                            }
                        }
                    }
                }

                Rectangle {
                    anchors.right: parent.right
                    width: 1
                    height: parent.height
                    color: Theme.colors.divider
                }
            }

            // Content
            Item {
                Layout.fillWidth: true
                Layout.fillHeight: true

                EmptyState {
                    anchors.centerIn: parent
                    visible: !win.drive
                    icon: "usb_drive"
                    title: Drives.connected ? I18n.tr("No stick plugged in") : I18n.tr("Connecting to the system…")
                    subtitle: Drives.connected ? I18n.tr("Plug in a USB stick or an SD card; it shows up here.") : ""
                }

                ColumnLayout {
                    anchors.fill: parent
                    anchors.margins: Theme.spacing.xl
                    spacing: Theme.spacing.lg
                    visible: !!win.drive

                    // The drive.
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: Theme.spacing.md

                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: Theme.spacing.xs

                            StyledText {
                                Layout.fillWidth: true
                                text: DrivesText.name(win.drive)
                                font.pixelSize: Theme.font.large
                                font.weight: Theme.font.weightSemiBold
                                elide: Text.ElideRight
                            }

                            Row {
                                spacing: Theme.spacing.xs

                                Chip {
                                    text: W.nominal(win.drive?.size ?? 0, Qt.locale())
                                }
                                Chip {
                                    text: win.drive?.device ?? ""
                                }
                                Chip {
                                    text: DrivesText.contents(win.drive)
                                    tint: win.ventoyStick ? Theme.colors.primary : Theme.colors.text
                                    strong: win.ventoyStick
                                }
                                Chip {
                                    visible: DrivesText.mounted(win.drive)
                                    text: I18n.tr("mounted")
                                }
                                Chip {
                                    visible: win.drive?.read_only ?? false
                                    icon: "lock"
                                    text: I18n.tr("write-protected")
                                    tint: Theme.pulse.warn
                                }
                            }
                        }

                        PillButton {
                            visible: (win.drive?.kind ?? "loop") !== "loop"
                            icon: "eject"
                            text: I18n.tr("Eject")
                            enabled: !Drives.busy
                            onClicked: Drives.eject()
                        }
                    }

                    Segmented {
                        options: [
                            {
                                k: "image",
                                t: win.ventoyStick ? I18n.tr("Images") : I18n.tr("Write image"),
                                i: "disc"
                            },
                            {
                                k: "format",
                                t: I18n.tr("Format"),
                                i: "erase"
                            }
                        ]
                        current: Drives.tab
                        onPicked: k => Drives.tab = k
                    }

                    // Images: the library (+ on a Ventoy stick what is on it).
                    ColumnLayout {
                        visible: Drives.tab === "image"
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        spacing: Theme.spacing.md

                        ColumnLayout {
                            visible: win.ventoyStick
                            Layout.fillWidth: true
                            spacing: Theme.spacing.xs

                            RowLayout {
                                Layout.fillWidth: true

                                SectionLabel {
                                    text: I18n.tr("On the stick")
                                }

                                Item {
                                    Layout.fillWidth: true
                                }

                                StyledText {
                                    text: win.drive?.ventoy?.mount ? I18n.tr("%1 free", W.size(win.drive.ventoy.free, Qt.locale())) : I18n.tr("not mounted")
                                    color: Theme.colors.textMuted
                                    font.pixelSize: Theme.font.small
                                }
                            }

                            StyledText {
                                visible: (win.drive?.ventoy?.files?.length ?? 0) === 0
                                text: win.drive?.ventoy?.mount ? I18n.tr("No images on it yet. Pick one below and copy it over.") : I18n.tr("Copying an image mounts the stick.")
                                color: Theme.colors.textMuted
                                wrapMode: Text.Wrap
                                Layout.fillWidth: true
                            }

                            Repeater {
                                model: win.drive?.ventoy?.files ?? []

                                RowLayout {
                                    required property var modelData

                                    Layout.fillWidth: true
                                    Layout.leftMargin: Theme.spacing.md
                                    spacing: Theme.spacing.md

                                    Monogram {
                                        size: 28
                                        system: modelData.system
                                        family: modelData.family
                                    }

                                    StyledText {
                                        Layout.fillWidth: true
                                        text: modelData.system + (modelData.version ? "  " + modelData.version : "") + "   ·   " + modelData.path
                                        elide: Text.ElideMiddle
                                    }

                                    StyledText {
                                        text: W.size(modelData.size, Qt.locale())
                                        color: Theme.colors.textMuted
                                        font.pixelSize: Theme.font.small
                                    }

                                    IconButton {
                                        icon: "delete"
                                        iconSize: Theme.icon.small
                                        iconColor: Theme.colors.textMuted
                                        enabled: !Drives.busy
                                        onClicked: win.confirm = {
                                            title: I18n.tr("Delete %1 from the stick?", modelData.file),
                                            text: I18n.tr("Only this file goes; the other images and Ventoy stay."),
                                            label: I18n.tr("Delete"),
                                            action: () => Drives.remove(modelData.path)
                                        }
                                    }
                                }
                            }

                            SectionLabel {
                                Layout.topMargin: Theme.spacing.md
                                text: I18n.tr("Image folder")
                            }
                        }

                        Item {
                            Layout.fillWidth: true
                            Layout.fillHeight: true

                            EmptyState {
                                anchors.centerIn: parent
                                visible: Drives.images.length === 0
                                icon: "disc"
                                title: I18n.tr("No images yet")
                                subtitle: I18n.tr("Put ISO or IMG files into %1, or drop them here.", Drives.hello.imageDirShown ?? "")
                            }

                            ListView {
                                id: list

                                anchors.fill: parent
                                clip: true
                                spacing: 2
                                model: Drives.images
                                boundsBehavior: Flickable.StopAtBounds

                                highlight: Rectangle {
                                    radius: Theme.radius.small
                                    color: Theme.colors.selected
                                    border.width: Theme.size.border
                                    border.color: Theme.colors.selectedRing
                                }
                                highlightMoveDuration: Theme.anim.normal
                                highlightResizeDuration: 0
                                currentIndex: Drives.images.findIndex(i => i.path === Drives.imagePath)

                                delegate: ImageRow {
                                    required property var modelData

                                    width: list.width
                                    image: modelData
                                    selected: modelData.path === Drives.imagePath
                                    fits: Drives.fits(modelData, win.drive)
                                    onPicked: Drives.imagePath = modelData.path
                                    onKeep: Drives.keep(modelData.path)
                                }
                            }
                        }

                        RowLayout {
                            Layout.fillWidth: true
                            spacing: Theme.spacing.md

                            Switch {
                                visible: !win.ventoyStick
                                checked: win.verify
                                onToggled: win.verify = !win.verify
                            }

                            StyledText {
                                visible: !win.ventoyStick
                                Layout.fillWidth: true
                                text: I18n.tr("Read back and compare afterwards")
                                color: Theme.colors.textMuted
                                wrapMode: Text.Wrap
                            }

                            Item {
                                visible: win.ventoyStick
                                Layout.fillWidth: true
                            }

                            PillButton {
                                style: win.ventoyStick ? "filled" : "danger"
                                icon: win.ventoyStick ? "download" : "disc"
                                text: win.ventoyStick ? I18n.tr("Copy onto the stick") : I18n.tr("Write to the stick")
                                enabled: !!Drives.image && !Drives.busy && !win.drive?.read_only && Drives.fits(Drives.image, win.drive)
                                onClicked: win.askWrite()
                            }
                        }
                    }

                    // Format.
                    ColumnLayout {
                        visible: Drives.tab === "format"
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        spacing: Theme.spacing.md

                        GridLayout {
                            Layout.fillWidth: true
                            columns: 2
                            columnSpacing: Theme.spacing.sm
                            rowSpacing: Theme.spacing.sm
                            opacity: win.ventoy ? 0.4 : 1
                            enabled: !win.ventoy
                            Behavior on opacity {
                                Anim {}
                            }

                            Repeater {
                                model: [
                                    {
                                        k: "fat32",
                                        t: "FAT32",
                                        d: I18n.tr("Works everywhere: Windows, macOS, TVs, car radios. Files up to 4 GB.")
                                    },
                                    {
                                        k: "exfat",
                                        t: "exFAT",
                                        d: I18n.tr("Big files, Windows, macOS and Linux. Not on very old devices.")
                                    },
                                    {
                                        k: "ntfs",
                                        t: "NTFS",
                                        d: I18n.tr("For Windows PCs. macOS only reads it.")
                                    },
                                    {
                                        k: "ext4",
                                        t: "ext4",
                                        d: I18n.tr("Linux only, with permissions. For backups and servers.")
                                    }
                                ]

                                Rectangle {
                                    id: card

                                    required property var modelData
                                    readonly property bool can: Drives.hello.formats?.[modelData.k]?.ok ?? true
                                    readonly property bool current: win.fs === modelData.k

                                    Layout.fillWidth: true
                                    Layout.preferredHeight: 84
                                    radius: Theme.radius.card
                                    color: current ? Theme.colors.selected : Theme.colors.tile
                                    border.width: Theme.size.border
                                    border.color: current ? Theme.colors.selectedRing : Theme.colors.outline
                                    opacity: can ? 1 : 0.5
                                    Behavior on color {
                                        ColorAnim {}
                                    }

                                    Clickable {
                                        radius: card.radius
                                        enabled: card.can
                                        onClicked: win.fs = card.modelData.k
                                    }

                                    ColumnLayout {
                                        anchors.fill: parent
                                        anchors.margins: Theme.spacing.md
                                        spacing: 2

                                        StyledText {
                                            text: card.modelData.t
                                            font.weight: Theme.font.weightSemiBold
                                        }

                                        StyledText {
                                            Layout.fillWidth: true
                                            Layout.fillHeight: true
                                            text: card.can ? card.modelData.d : I18n.tr("Needs %1 (package ntfs-3g).", Drives.hello.formats?.[card.modelData.k]?.missing || "mkfs." + card.modelData.k)
                                            color: Theme.colors.textMuted
                                            font.pixelSize: Theme.font.small
                                            wrapMode: Text.Wrap
                                        }
                                    }
                                }
                            }
                        }

                        // Ventoy: many ISOs on one stick.
                        Rectangle {
                            Layout.fillWidth: true
                            Layout.preferredHeight: ventoyRow.implicitHeight + 2 * Theme.spacing.md
                            radius: Theme.radius.card
                            color: win.ventoy ? Theme.colors.selected : Theme.colors.tile
                            border.width: Theme.size.border
                            border.color: win.ventoy ? Theme.colors.selectedRing : Theme.colors.outline
                            opacity: Drives.hello.ventoy ? 1 : 0.5
                            Behavior on color {
                                ColorAnim {}
                            }

                            RowLayout {
                                id: ventoyRow

                                anchors.fill: parent
                                anchors.margins: Theme.spacing.md
                                spacing: Theme.spacing.md

                                ColumnLayout {
                                    Layout.fillWidth: true
                                    spacing: 2

                                    StyledText {
                                        text: I18n.tr("Install Ventoy (multiboot)")
                                        font.weight: Theme.font.weightSemiBold
                                    }

                                    StyledText {
                                        Layout.fillWidth: true
                                        text: Drives.hello.ventoy ? I18n.tr("Many ISOs on one stick: copy them on like files and pick one when booting. The stick stays usable for other files (exFAT).") : I18n.tr("Needs Ventoy (package ventoy-bin).")
                                        color: Theme.colors.textMuted
                                        font.pixelSize: Theme.font.small
                                        wrapMode: Text.Wrap
                                    }
                                }

                                Switch {
                                    enabled: Drives.hello.ventoy ?? false
                                    checked: win.ventoy
                                    onToggled: win.ventoy = !win.ventoy
                                }
                            }
                        }

                        RowLayout {
                            Layout.fillWidth: true
                            spacing: Theme.spacing.md

                            StyledText {
                                text: I18n.tr("Name")
                            }

                            TextField {
                                id: label

                                Layout.fillWidth: true
                                placeholder: win.ventoy ? "Ventoy" : win.fs === "fat32" ? "USB-STICK" : I18n.tr("USB stick")
                            }
                        }

                        RowLayout {
                            Layout.fillWidth: true
                            visible: !win.ventoy
                            spacing: Theme.spacing.md

                            Switch {
                                checked: win.erase
                                onToggled: win.erase = !win.erase
                            }

                            StyledText {
                                Layout.fillWidth: true
                                text: I18n.tr("Erase thoroughly: overwrite every byte with zeros first (slow; old data can't be recovered)")
                                color: Theme.colors.textMuted
                                wrapMode: Text.Wrap
                            }
                        }

                        Item {
                            Layout.fillHeight: true
                        }

                        RowLayout {
                            Layout.fillWidth: true

                            Item {
                                Layout.fillWidth: true
                            }

                            PillButton {
                                style: "danger"
                                icon: "erase"
                                text: win.ventoy ? I18n.tr("Install Ventoy") : I18n.tr("Format")
                                enabled: !Drives.busy && !win.drive?.read_only && (win.ventoy ? (Drives.hello.ventoy ?? false) : (Drives.hello.formats?.[win.fs]?.ok ?? true))
                                onClicked: win.askFormat()
                            }
                        }
                    }
                }
            }
        }

        // Drop hint.
        Rectangle {
            anchors.fill: parent
            anchors.margins: Theme.spacing.md
            visible: drop.containsDrag
            radius: Theme.radius.panel
            color: Theme.withAlpha(Theme.colors.primary, 0.08)
            border.width: 2
            border.color: Theme.colors.selectedRing

            StyledText {
                anchors.centerIn: parent
                text: I18n.tr("Drop images to offer them")
                font.pixelSize: Theme.font.large
            }
        }

        Scrim {
            id: ask

            property var shownC: null
            readonly property var c: win.confirm
            onCChanged: if (c) shownC = c

            anchors.fill: parent
            open: c !== null
            onDismissed: win.confirm = null

            ColumnLayout {
                width: parent.width
                spacing: Theme.spacing.md

                StyledText {
                    Layout.fillWidth: true
                    text: ask.shownC?.title ?? ""
                    font.pixelSize: Theme.font.large
                    font.weight: Theme.font.weightSemiBold
                    wrapMode: Text.Wrap
                }

                StyledText {
                    Layout.fillWidth: true
                    text: ask.shownC?.text ?? ""
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
                        onClicked: win.confirm = null
                    }

                    PillButton {
                        style: "danger"
                        text: ask.shownC?.label ?? ""
                        onClicked: {
                            const a = ask.shownC?.action;
                            win.confirm = null;
                            if (a)
                                a();
                        }
                    }
                }
            }
        }

        JobView {
            anchors.fill: parent
        }
    }
}
