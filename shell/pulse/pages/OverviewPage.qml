import QtQuick
import QtQuick.Layouts
import qs
import qs.components
import qs.pulse
import "../Fmt.js" as Fmt
import "../Model.js" as Model

// Bento overview: health, live tiles per resource, top consumers, activity.
Flickable {
    id: page

    readonly property var f: Pulse.frame
    readonly property var loc: Qt.locale()
    readonly property int cols: width >= 1150 ? 4 : width >= 760 ? 2 : 1
    // Hottest sensors first, by label so a row keeps its sensor.
    readonly property var hottest: {
        const all = (f?.sensors ?? []).slice().sort((a, b) => b.celsius - a.celsius).slice(0, 6);
        return all.sort((a, b) => (a.chip + a.label).localeCompare(b.chip + b.label));
    }

    contentHeight: grid.implicitHeight + 2 * Theme.spacing.xl
    clip: true
    boundsBehavior: Flickable.StopAtBounds

    component Label: StyledText {
        color: Theme.colors.textMuted
        font.pixelSize: Theme.font.section
        font.weight: Theme.font.weightMedium
        font.letterSpacing: Theme.font.labelLetterSpacing
        font.capitalization: Font.AllUppercase
    }

    component Big: Counter {
        font.pixelSize: 30
        font.weight: Theme.font.weightSemiBold
    }

    GridLayout {
        id: grid

        x: Theme.spacing.xl
        y: Theme.spacing.xl
        width: page.width - 2 * Theme.spacing.xl
        columns: page.cols
        columnSpacing: Theme.spacing.md
        rowSpacing: Theme.spacing.md

        // Health
        Tile {
            order: 0
            Layout.columnSpan: Math.min(2, page.cols)
            Layout.fillWidth: true
            Layout.preferredHeight: Math.max(236, healthCol.implicitHeight + 2 * Theme.spacing.lg)
            clickable: true
            onClicked: PulseUi.show("diagnosis")

            RowLayout {
                anchors.fill: parent
                spacing: Theme.spacing.xl

                Ring {
                    Layout.preferredWidth: 150
                    Layout.preferredHeight: 150
                    Layout.alignment: Qt.AlignVCenter
                    value: Pulse.score
                    thickness: 12
                    color: Pulse.score >= 85 ? Theme.pulse.ok : Pulse.score >= 60 ? Theme.pulse.warn : Theme.pulse.crit
                    alert: Pulse.findings.some(x => x.severity === "critical")

                    ColumnLayout {
                    id: healthCol

                    Layout.preferredWidth: 0
                        anchors.centerIn: parent
                        spacing: -4

                        Counter {
                            Layout.alignment: Qt.AlignHCenter
                            value: Pulse.score
                            font.pixelSize: 40
                            font.weight: Theme.font.weightSemiBold
                        }

                        StyledText {
                            Layout.alignment: Qt.AlignHCenter
                            text: I18n.tr("Health")
                            color: Theme.colors.textMuted
                            font.pixelSize: Theme.font.small
                        }
                    }
                }

                ColumnLayout {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    spacing: Theme.spacing.sm

                    Label {
                        text: I18n.tr("System health")
                    }

                    SwapText {
                        Layout.fillWidth: true
                        value: Pulse.findings.length === 0 ? I18n.tr("Everything runs smoothly") : Words.title(Pulse.findings[0])
                        font.pixelSize: Theme.font.large
                        font.weight: Theme.font.weightSemiBold
                        wrapMode: Text.Wrap
                        maximumLineCount: 2
                    }

                    StyledText {
                        Layout.fillWidth: true
                        text: Pulse.findings.length === 0 ? I18n.tr("No problems found. Pulse keeps watching in the background while this window is open.") : Words.text(Pulse.findings[0])
                        color: Theme.colors.textMuted
                        wrapMode: Text.Wrap
                        maximumLineCount: 3
                    }

                    Item {
                        Layout.fillHeight: true
                    }

                    Flow {
                        Layout.fillWidth: true
                        spacing: Theme.spacing.xs

                        Repeater {
                            model: Pulse.findings.slice(1, 5)

                            Chip {
                                required property var modelData

                                text: Words.title(modelData)
                                icon: Words.severityIcon(modelData.severity)
                                tint: Words.severityColor(modelData.severity)
                            }
                        }
                    }
                }
            }
        }

        // CPU
        Tile {
            order: 1
            Layout.fillWidth: true
            Layout.preferredHeight: 236
            clickable: true
            onClicked: PulseUi.showPerf("cpu")

            ColumnLayout {
                anchors.fill: parent
                spacing: Theme.spacing.xs

                RowLayout {
                    Layout.fillWidth: true

                    MaterialIcon {
                        icon: "pulse_cpu"
                        color: Theme.pulse.cpu
                    }

                    Label {
                        Layout.fillWidth: true
                        text: I18n.tr("Processor")
                    }

                    Chip {
                        visible: (page.f?.cpu.temp ?? null) !== null
                        text: Fmt.celsius(page.f?.cpu.temp ?? 0, page.loc)
                        tint: (page.f?.cpu.temp ?? 0) >= 90 ? Theme.pulse.crit : Theme.colors.textMuted
                    }
                }

                RowLayout {
                    Big {
                        value: page.f?.cpu.usage ?? 0
                        format: v => Fmt.percent(v, page.loc, 0)
                    }

                    StyledText {
                        Layout.alignment: Qt.AlignBottom
                        Layout.bottomMargin: 6
                        text: Fmt.mhz(page.f?.cpu.avgMhz ?? 0, page.loc)
                        color: Theme.colors.textMuted
                        font.pixelSize: Theme.font.small
                    }
                }

                Graph {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    values: Pulse.series("cpu", 62)
                    max: 100
                    grid: false
                    color: Theme.pulse.cpu
                    format: v => Fmt.percent(v, page.loc, 0)
                }

                // One bar per thread, like an equalizer.
                Row {
                    Layout.fillWidth: true
                    Layout.preferredHeight: 26
                    spacing: 2

                    Repeater {
                        model: page.f?.cpu.cores.length ?? 0

                        Item {
                            required property int index

                            width: (parent.width - (parent.children.length - 1) * 2) / Math.max(1, page.f?.cpu.cores.length ?? 1)
                            height: 26

                            Rectangle {
                                anchors.fill: parent
                                radius: 2
                                color: Theme.withAlpha(Theme.colors.text, 0.06)
                            }

                            Rectangle {
                                anchors.bottom: parent.bottom
                                width: parent.width
                                radius: 2
                                height: Math.max(2, parent.height * (page.f?.cpu.cores[parent.index] ?? 0) / 100)
                                color: Theme.pulse.cpu
                                opacity: 0.45 + 0.55 * (page.f?.cpu.cores[parent.index] ?? 0) / 100
                                Behavior on height {
                                    SpringAnim {
                                        duration: Theme.anim.slow
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Memory
        Tile {
            id: memTile

            order: 2
            Layout.fillWidth: true
            Layout.preferredHeight: 236
            clickable: true
            onClicked: PulseUi.showPerf("memory")

            readonly property var m: page.f?.memory ?? null

            ColumnLayout {
                anchors.fill: parent
                spacing: Theme.spacing.xs

                RowLayout {
                    Layout.fillWidth: true

                    MaterialIcon {
                        icon: "pulse_memory"
                        color: Theme.pulse.memory
                    }

                    Label {
                        Layout.fillWidth: true
                        text: I18n.tr("Memory")
                    }

                    Chip {
                        visible: (memTile.m?.pressure ?? 0) >= 5
                        text: I18n.tr("Pressure %1", Fmt.percent(memTile.m?.pressure ?? 0, page.loc))
                        tint: Theme.pulse.warn
                    }
                }

                RowLayout {
                    Big {
                        value: memTile.m?.used ?? 0
                        format: v => Fmt.bytes(v, page.loc)
                    }

                    StyledText {
                        Layout.alignment: Qt.AlignBottom
                        Layout.bottomMargin: 6
                        text: I18n.tr("of %1", Fmt.bytes(memTile.m?.total ?? 0, page.loc))
                        color: Theme.colors.textMuted
                        font.pixelSize: Theme.font.small
                    }
                }

                Graph {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    values: Pulse.series("mem", 62)
                    max: 100
                    grid: false
                    color: Theme.pulse.memory
                    format: v => Fmt.percent(v, page.loc, 0)
                }

                Meter {
                    readonly property var m: memTile.m
                    Layout.fillWidth: true
                    implicitHeight: 8
                    max: m?.total ?? 1
                    segments: m ? [
                        {
                            value: m.used,
                            color: Theme.pulse.memory
                        },
                        {
                            value: m.cache,
                            color: Theme.withAlpha(Theme.pulse.memory, 0.35)
                        }
                    ] : []
                }

                RowLayout {
                    readonly property var m: memTile.m
                    Layout.fillWidth: true

                    StyledText {
                        Layout.fillWidth: true
                        text: I18n.tr("Cache %1", Fmt.bytes(memTile.m?.cache ?? 0, page.loc))
                        color: Theme.colors.textMuted
                        font.pixelSize: Theme.font.small
                    }

                    StyledText {
                        visible: (memTile.m?.swapTotal ?? 0) > 0
                        text: I18n.tr("Swap %1", Fmt.bytes(memTile.m?.swapUsed ?? 0, page.loc))
                        color: Theme.colors.textMuted
                        font.pixelSize: Theme.font.small
                    }
                }
            }
        }

        // GPUs
        Tile {
            order: 3
            Layout.fillWidth: true
            Layout.preferredHeight: 196
            clickable: (page.f?.gpus.length ?? 0) > 0
            onClicked: PulseUi.showPerf(page.f.gpus[0].card)

            ColumnLayout {
                anchors.fill: parent
                spacing: Theme.spacing.sm

                RowLayout {
                    MaterialIcon {
                        icon: "pulse_gpu"
                        color: Theme.pulse.gpu
                    }

                    Label {
                        text: I18n.tr("Graphics")
                    }
                }

                Repeater {
                    model: page.f?.gpus.length ?? 0

                    ColumnLayout {
                        required property int index
                        readonly property var modelData: page.f?.gpus[index] ?? ({})

                        Layout.fillWidth: true
                        spacing: 4

                        RowLayout {
                            Layout.fillWidth: true

                            StyledText {
                                Layout.fillWidth: true
                                text: parent.parent.modelData.name
                                font.weight: Theme.font.weightMedium
                            }

                            Chip {
                                visible: parent.parent.modelData.asleep
                                text: I18n.tr("Asleep")
                                icon: "bedtime"
                                tint: Theme.pulse.ok
                            }

                            Counter {
                                visible: !parent.parent.modelData.asleep
                                value: parent.parent.modelData.busy ?? 0
                                format: v => Fmt.percent(v, page.loc, 0)
                                font.weight: Theme.font.weightMedium
                            }
                        }

                        Meter {
                            Layout.fillWidth: true
                            value: parent.modelData.busy ?? 0
                            color: Theme.pulse.gpu
                            opacity: parent.modelData.asleep ? 0.4 : 1
                        }

                        StyledText {
                            Layout.fillWidth: true
                            visible: text !== ""
                            color: Theme.colors.textMuted
                            font.pixelSize: Theme.font.small
                            text: {
                                const g = parent.modelData;
                                const p = [];
                                if (g.vramTotal)
                                    p.push(I18n.tr("VRAM %1 / %2", Fmt.bytes(g.vramUsed, page.loc), Fmt.bytes(g.vramTotal, page.loc)));
                                if (g.temp)
                                    p.push(Fmt.celsius(g.temp, page.loc));
                                if (g.watts)
                                    p.push(Fmt.watts(g.watts, page.loc));
                                return p.join("  ·  ");
                            }
                        }
                    }
                }

                Item {
                    Layout.fillHeight: true
                }
            }
        }

        // Disk
        Tile {
            id: diskTile

            readonly property var d: (page.f?.disks ?? []).find(x => x.mounts.some(m => m.path === "/")) ?? (page.f?.disks ?? [])[0] ?? null

            order: 4
            Layout.fillWidth: true
            Layout.preferredHeight: 196
            clickable: d !== null
            onClicked: PulseUi.showPerf(d.name)

            ColumnLayout {
                anchors.fill: parent
                spacing: Theme.spacing.xs

                RowLayout {
                    MaterialIcon {
                        icon: "storage"
                        color: Theme.pulse.disk
                    }

                    Label {
                        Layout.fillWidth: true
                        text: I18n.tr("Disk")
                    }

                    Counter {
                        value: diskTile.d?.busy ?? 0
                        format: v => I18n.tr("%1 busy", Fmt.percent(v, page.loc, 0))
                        color: Theme.colors.textMuted
                        font.pixelSize: Theme.font.small
                    }
                }

                RowLayout {
                    Layout.fillWidth: true
                    spacing: Theme.spacing.lg

                    ColumnLayout {
                        spacing: 0

                        StyledText {
                            text: I18n.tr("Read")
                            color: Theme.colors.textMuted
                            font.pixelSize: Theme.font.small
                        }

                        Counter {
                            value: diskTile.d?.readBps ?? 0
                            format: v => Fmt.rate(v, page.loc)
                            color: Theme.pulse.disk
                            font.weight: Theme.font.weightMedium
                        }
                    }

                    ColumnLayout {
                        spacing: 0

                        StyledText {
                            text: I18n.tr("Write")
                            color: Theme.colors.textMuted
                            font.pixelSize: Theme.font.small
                        }

                        Counter {
                            value: diskTile.d?.writeBps ?? 0
                            format: v => Fmt.rate(v, page.loc)
                            color: Theme.pulse.diskWrite
                            font.weight: Theme.font.weightMedium
                        }
                    }
                }

                Graph {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    values: diskTile.d ? Pulse.series("disk.r." + diskTile.d.name, 62) : []
                    values2: diskTile.d ? Pulse.series("disk.w." + diskTile.d.name, 62) : []
                    minMax: 1024 * 1024
                    grid: false
                    color: Theme.pulse.disk
                    format: v => Fmt.rate(v, page.loc)
                }

                Meter {
                    readonly property var root_: diskTile.d?.mounts.find(m => m.path === "/") ?? diskTile.d?.mounts[0] ?? null
                    Layout.fillWidth: true
                    max: root_?.total ?? 1
                    value: root_?.used ?? 0
                    color: (root_ && root_.used / root_.total > 0.9) ? Theme.pulse.crit : Theme.pulse.disk
                }
            }
        }

        // Network
        Tile {
            id: netTile

            readonly property var n: (page.f?.net ?? []).find(x => x.up && x.kind !== "virtual") ?? null

            order: 5
            Layout.fillWidth: true
            Layout.preferredHeight: 196
            clickable: n !== null
            onClicked: {
                PulseUi.device = "net:" + n.iface;
                PulseUi.show("performance");
            }

            ColumnLayout {
                anchors.fill: parent
                spacing: Theme.spacing.xs

                RowLayout {
                    MaterialIcon {
                        icon: netTile.n?.kind === "wifi" ? "wifi" : "ethernet"
                        color: Theme.pulse.net
                    }

                    Label {
                        Layout.fillWidth: true
                        text: I18n.tr("Network")
                    }

                    StyledText {
                        text: netTile.n?.iface ?? I18n.tr("Offline")
                        color: Theme.colors.textMuted
                        font.pixelSize: Theme.font.small
                    }
                }

                RowLayout {
                    Layout.fillWidth: true
                    spacing: Theme.spacing.lg

                    ColumnLayout {
                        spacing: 0

                        StyledText {
                            text: I18n.tr("Down")
                            color: Theme.colors.textMuted
                            font.pixelSize: Theme.font.small
                        }

                        Counter {
                            value: netTile.n?.rxBps ?? 0
                            format: v => Fmt.bits(v, page.loc)
                            color: Theme.pulse.net
                            font.weight: Theme.font.weightMedium
                        }
                    }

                    ColumnLayout {
                        spacing: 0

                        StyledText {
                            text: I18n.tr("Up")
                            color: Theme.colors.textMuted
                            font.pixelSize: Theme.font.small
                        }

                        Counter {
                            value: netTile.n?.txBps ?? 0
                            format: v => Fmt.bits(v, page.loc)
                            color: Theme.pulse.netUp
                            font.weight: Theme.font.weightMedium
                        }
                    }
                }

                Graph {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    values: netTile.n ? Pulse.series("net.rx." + netTile.n.iface, 62) : []
                    values2: netTile.n ? Pulse.series("net.tx." + netTile.n.iface, 62) : []
                    minMax: 128 * 1024
                    grid: false
                    color: Theme.pulse.net
                    color2: Theme.pulse.netUp
                    format: v => Fmt.bits(v, page.loc)
                }
            }
        }

        // Power
        Tile {
            id: powerTile

            readonly property var p: page.f?.power ?? null
            readonly property var bat: p?.batteries[0] ?? null

            order: 6
            Layout.fillWidth: true
            Layout.preferredHeight: 196

            ColumnLayout {
                anchors.fill: parent
                spacing: Theme.spacing.sm

                RowLayout {
                    MaterialIcon {
                        icon: powerTile.bat ? (powerTile.bat.status === "charging" ? "battery_charging_full" : "battery_full") : "power_settings_new"
                        color: Theme.pulse.power
                    }

                    Label {
                        Layout.fillWidth: true
                        text: I18n.tr("Power")
                    }

                    StyledText {
                        text: powerTile.p?.onAc ? I18n.tr("Plugged in") : I18n.tr("On battery")
                        color: Theme.colors.textMuted
                        font.pixelSize: Theme.font.small
                    }
                }

                RowLayout {
                    visible: powerTile.bat !== null

                    Big {
                        value: powerTile.bat?.percent ?? 0
                        format: v => Fmt.percent(v, page.loc, 0)
                    }

                    StyledText {
                        Layout.alignment: Qt.AlignBottom
                        Layout.bottomMargin: 6
                        color: Theme.colors.textMuted
                        font.pixelSize: Theme.font.small
                        text: {
                            const b = powerTile.bat;
                            if (!b)
                                return "";
                            const parts = [];
                            if (b.watts > 0.3)
                                parts.push(Fmt.watts(b.watts, page.loc));
                            if (b.secondsLeft)
                                parts.push(b.status === "charging" ? I18n.tr("full in %1", I18n.duration(b.secondsLeft)) : I18n.tr("%1 left", I18n.duration(b.secondsLeft)));
                            return parts.join("  ·  ");
                        }
                    }
                }

                Meter {
                    visible: powerTile.bat !== null
                    Layout.fillWidth: true
                    value: powerTile.bat?.percent ?? 0
                    color: (powerTile.bat?.percent ?? 100) < 15 ? Theme.pulse.crit : Theme.pulse.power
                }

                Item {
                    Layout.fillHeight: true
                }

                // Power profile switcher (power-profiles-daemon).
                Rectangle {
                    visible: (page.f?.powerProfile ?? "") !== ""
                    Layout.fillWidth: true
                    implicitHeight: 36
                    radius: Theme.radius.small
                    color: Theme.withAlpha(Theme.colors.text, 0.05)

                    readonly property var profiles: ["power-saver", "balanced", "performance"]
                    readonly property int active: profiles.indexOf(page.f?.powerProfile ?? "")

                    Rectangle {
                        visible: parent.active >= 0
                        x: 3 + parent.active * (parent.width - 6) / 3
                        y: 3
                        width: (parent.width - 6) / 3
                        height: parent.height - 6
                        radius: Theme.radius.small - 2
                        color: Theme.colors.selected
                        border.width: 1
                        border.color: Theme.colors.selectedRing
                        Behavior on x {
                            SpringAnim {}
                        }
                    }

                    Row {
                        anchors.fill: parent
                        anchors.margins: 3

                        Repeater {
                            model: parent.parent.profiles

                            Item {
                                required property string modelData
                                required property int index

                                width: parent.width / 3
                                height: parent.height

                                Clickable {
                                    radius: Theme.radius.small
                                    onClicked: Pulse.setProfile(parent.modelData)
                                }

                                MaterialIcon {
                                    anchors.centerIn: parent
                                    icon: ["profile_saver", "profile_balanced", "profile_performance"][parent.index]
                                    size: Theme.icon.small
                                    color: parent.index === parent.parent.parent.active ? Theme.colors.primary : Theme.colors.textMuted
                                }
                            }
                        }
                    }
                }
            }
        }

        // Top consumers
        Tile {
            id: topTile

            property string by: "cpu"
            // Averaged over the last 30 seconds, so the order is calm.
            function usage(a: var): real {
                return Pulse.mean(({
                        cpu: "app.cpu.",
                        mem: "app.mem.",
                        gpu: "app.gpu.",
                        disk: "app.disk."
                    })[by] + a.key, 30);
            }
            readonly property var sorted: {
                Pulse.revision;
                return Pulse.apps.filter(a => a.kind !== "kernel").map(a => ({
                            key: a.key,
                            v: usage(a)
                        })).filter(x => x.v > 0).sort((x, y) => y.v - x.v).slice(0, 7);
            }
            onSortedChanged: Model.sync(topModel, sorted.map(a => a.key))

            order: 7
            Layout.columnSpan: Math.min(2, page.cols)
            Layout.fillWidth: true
            Layout.preferredHeight: 330

            ListModel {
                id: topModel
            }

            ColumnLayout {
                anchors.fill: parent
                spacing: Theme.spacing.sm

                RowLayout {
                    Layout.fillWidth: true

                    Label {
                        Layout.fillWidth: true
                        text: I18n.tr("Using the most")
                    }

                    Repeater {
                        model: [
                            {
                                k: "cpu",
                                t: I18n.tr("CPU")
                            },
                            {
                                k: "mem",
                                t: I18n.tr("Memory")
                            },
                            {
                                k: "gpu",
                                t: I18n.tr("GPU")
                            },
                            {
                                k: "disk",
                                t: I18n.tr("Disk")
                            }
                        ]

                        Rectangle {
                            required property var modelData

                            implicitWidth: segText.implicitWidth + 2 * Theme.spacing.md
                            implicitHeight: 26
                            radius: 13
                            color: topTile.by === modelData.k ? Theme.colors.selected : "transparent"
                            border.width: topTile.by === modelData.k ? 1 : 0
                            border.color: Theme.colors.selectedRing
                            Behavior on color {
                                ColorAnim {}
                            }

                            Clickable {
                                radius: 13
                                onClicked: topTile.by = parent.modelData.k
                            }

                            StyledText {
                                id: segText

                                anchors.centerIn: parent
                                text: parent.modelData.t
                                font.pixelSize: Theme.font.small
                            }
                        }
                    }
                }

                ListView {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    model: topModel
                    interactive: false
                    spacing: 2

                    move: Transition {
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
                    moveDisplaced: Transition {
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
                    add: Transition {
                        Anim {
                            property: "opacity"
                            from: 0
                            to: 1
                        }
                        Anim {
                            property: "x"
                            from: 24
                            to: 0
                            easing.bezierCurve: Theme.anim.emphasizedDecel
                        }
                    }
                    remove: Transition {
                        Anim {
                            property: "opacity"
                            to: 0
                            duration: Theme.anim.fast
                        }
                    }

                    delegate: Item {
                        id: row

                        required property string key
                        readonly property var app: Pulse.app(key)
                        readonly property real value: topTile.sorted.find(x => x.key === row.key)?.v ?? 0
                        readonly property real best: topTile.sorted.length > 0 ? topTile.sorted[0].v : 1

                        width: ListView.view.width
                        height: 36

                        Clickable {
                            radius: Theme.radius.small
                            onClicked: PulseUi.showApp(row.key)
                        }

                        RowLayout {
                            anchors.fill: parent
                            anchors.leftMargin: Theme.spacing.xs
                            anchors.rightMargin: Theme.spacing.xs
                            spacing: Theme.spacing.md

                            AppIcon {
                                icon: row.app?.icon ?? ""
                                name: row.app?.name ?? ""
                                fallback: row.app?.kind === "task" ? "utilities-terminal" : "application-x-executable"
                                size: 22
                            }

                            StyledText {
                                Layout.preferredWidth: 180
                                text: row.app?.name ?? ""
                            }

                            Meter {
                                Layout.fillWidth: true
                                value: row.value
                                max: Math.max(row.best, 1e-9)
                                color: ({
                                        cpu: Theme.pulse.cpu,
                                        mem: Theme.pulse.memory,
                                        gpu: Theme.pulse.gpu,
                                        disk: Theme.pulse.disk
                                    })[topTile.by]
                            }

                            Counter {
                                Layout.preferredWidth: 80
                                horizontalAlignment: Text.AlignRight
                                value: row.value
                                format: v => topTile.by === "mem" ? Fmt.bytes(v, page.loc) : topTile.by === "disk" ? Fmt.rate(v, page.loc) : Fmt.percent(v, page.loc)
                                font.weight: Theme.font.weightMedium
                            }
                        }
                    }
                }
            }
        }

        // Activity
        Tile {
            order: 8
            Layout.fillWidth: true
            Layout.preferredHeight: 330
            clickable: true
            onClicked: PulseUi.show("activity")

            ColumnLayout {
                anchors.fill: parent
                spacing: Theme.spacing.sm

                Label {
                    text: I18n.tr("Activity")
                }

                StyledText {
                    visible: Pulse.events.length === 0
                    Layout.fillWidth: true
                    text: I18n.tr("Apps that start, close, crash or hang show up here.")
                    color: Theme.colors.textMuted
                    wrapMode: Text.Wrap
                }

                Repeater {
                    model: Pulse.events.slice(0, 7)

                    RowLayout {
                        required property var modelData

                        Layout.fillWidth: true
                        spacing: Theme.spacing.sm

                        Rectangle {
                            implicitWidth: 24
                            implicitHeight: 24
                            radius: 12
                            color: Theme.withAlpha(Words.eventColor(parent.modelData.kind), 0.16)

                            MaterialIcon {
                                anchors.centerIn: parent
                                icon: Words.eventIcon(parent.parent.modelData.kind)
                                size: 12
                                color: Words.eventColor(parent.parent.modelData.kind)
                            }
                        }

                        StyledText {
                            Layout.fillWidth: true
                            text: Words.event(parent.modelData)
                            font.pixelSize: Theme.font.small + 1
                        }

                        StyledText {
                            text: Words.ago(parent.modelData.t / 1000)
                            color: Theme.colors.textMuted
                            font.pixelSize: Theme.font.small
                        }
                    }
                }

                Item {
                    Layout.fillHeight: true
                }
            }
        }

        // Sensors
        Tile {
            order: 9
            Layout.fillWidth: true
            Layout.preferredHeight: 330
            clickable: true
            onClicked: PulseUi.showPerf("sensors")

            ColumnLayout {
                anchors.fill: parent
                spacing: Theme.spacing.sm

                RowLayout {
                    MaterialIcon {
                        icon: "pulse_thermo"
                        color: Theme.pulse.sensor
                    }

                    Label {
                        text: I18n.tr("Sensors")
                    }
                }

                Repeater {
                    model: page.hottest.length

                    ColumnLayout {
                        required property int index
                        readonly property var modelData: page.hottest[index] ?? ({ label: "", celsius: 0 })

                        Layout.fillWidth: true
                        spacing: 2

                        RowLayout {
                            Layout.fillWidth: true

                            StyledText {
                                Layout.fillWidth: true
                                text: parent.parent.modelData.label
                                font.pixelSize: Theme.font.small + 1
                            }

                            Counter {
                                value: parent.parent.modelData.celsius
                                format: v => Fmt.celsius(v, page.loc)
                                font.pixelSize: Theme.font.small + 1
                                font.weight: Theme.font.weightMedium
                            }
                        }

                        Meter {
                            Layout.fillWidth: true
                            implicitHeight: 4
                            max: parent.modelData.crit ?? 100
                            value: parent.modelData.celsius
                            color: parent.modelData.celsius >= (parent.modelData.crit ?? 100) - 10 ? Theme.pulse.crit : Theme.pulse.sensor
                        }
                    }
                }

                Repeater {
                    model: page.f?.fans.length ?? 0

                    RowLayout {
                        id: fanRow

                        required property int index
                        readonly property var modelData: page.f?.fans[index] ?? ({ label: "", rpm: 0 })

                        Layout.fillWidth: true

                        MaterialIcon {
                            icon: "pulse_fan"
                            size: Theme.icon.small
                            color: Theme.colors.textMuted
                            // Spins with the fan: one turn per 6000 rpm-seconds.
                            RotationAnimation on rotation {
                                running: fanRow.modelData.rpm > 0 && Theme.anim.normal > 0
                                loops: Animation.Infinite
                                from: 0
                                to: 360
                                duration: Math.max(300, 6000000 / Math.max(1, fanRow.modelData.rpm))
                            }
                        }

                        StyledText {
                            Layout.fillWidth: true
                            text: parent.modelData.label
                            font.pixelSize: Theme.font.small + 1
                        }

                        StyledText {
                            text: I18n.tr("%1 rpm", parent.modelData.rpm)
                            font.pixelSize: Theme.font.small + 1
                        }
                    }
                }

                Item {
                    Layout.fillHeight: true
                }
            }
        }
    }
}
