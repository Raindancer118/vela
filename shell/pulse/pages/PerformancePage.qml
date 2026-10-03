import QtQuick
import QtQuick.Layouts
import Quickshell
import qs
import qs.components
import qs.pulse
import "../Fmt.js" as Fmt

// Mission Center / Windows style: every device with a live mini graph on
// the left, the chosen one in depth on the right.
Item {
    id: page

    readonly property var f: Pulse.frame
    readonly property var loc: Qt.locale()
    readonly property int pts: Pulse.longRange ? Pulse.longPoints() : Pulse.range
    // A series for the big graphs: live, or the daemon's recording.
    function ser(key: string): var {
        return Pulse.longRange ? Pulse.longSeries(key) : Pulse.series(key, pts + 2);
    }
    // VELA_PULSE_PERCORE=1: test hook for screenshots.
    property bool perCore: Quickshell.env("VELA_PULSE_PERCORE") === "1"
    readonly property bool showTemp: Pulse.cpuTemp && page.f?.cpu.temp != null
    // Shared °C scale: 100 or the chip's critical mark.
    readonly property real tempTop: Math.max(100, page.f?.cpu.tempCrit ?? 0)

    readonly property var devices: {
        const f = page.f;
        if (!f)
            return [];
        const d = [
            {
                id: "cpu",
                icon: "pulse_cpu",
                name: I18n.tr("Processor"),
                sub: Fmt.percent(f.cpu.usage, loc, 0) + "  " + Fmt.mhz(f.cpu.avgMhz, loc),
                series: "cpu",
                max: 100,
                color: Theme.pulse.cpu
            },
            {
                id: "memory",
                icon: "pulse_memory",
                name: I18n.tr("Memory"),
                sub: Fmt.bytes(f.memory.used, loc) + " / " + Fmt.bytes(f.memory.total, loc),
                series: "mem",
                max: 100,
                color: Theme.pulse.memory
            }
        ];
        for (const g of f.gpus)
            d.push({
                id: "gpu:" + g.card,
                icon: "pulse_gpu",
                name: g.name,
                sub: g.asleep ? I18n.tr("Asleep") : Fmt.percent(g.busy ?? 0, loc, 0) + (g.temp ? "  " + Fmt.celsius(g.temp, loc) : ""),
                series: "gpu." + g.card,
                max: 100,
                color: Theme.pulse.gpu
            });
        for (const k of f.disks)
            d.push({
                id: "disk:" + k.name,
                icon: "storage",
                name: I18n.tr("Disk %1", k.name),
                sub: Fmt.percent(k.busy, loc, 0) + "  " + Fmt.rate(k.readBps + k.writeBps, loc),
                series: "disk.busy." + k.name,
                max: 100,
                color: Theme.pulse.disk
            });
        for (const n of f.net)
            if (n.kind !== "virtual" && (n.up || n.rxTotal > 0))
                d.push({
                    id: "net:" + n.iface,
                    icon: n.kind === "wifi" ? "wifi" : n.kind === "vpn" ? "vpn" : "ethernet",
                    name: n.kind === "wifi" ? I18n.tr("Wi-Fi") : n.kind === "vpn" ? I18n.tr("VPN") : I18n.tr("Ethernet"),
                    sub: n.iface + "  ↓ " + Fmt.bits(n.rxBps, loc),
                    series: "net.rx." + n.iface,
                    max: 0,
                    color: Theme.pulse.net
                });
        if (f.power.batteries.length > 0)
            d.push({
                id: "power",
                icon: "battery_full",
                name: I18n.tr("Battery"),
                sub: Fmt.percent(f.power.batteries[0].percent, loc, 0) + (f.power.batteries[0].watts > 0.3 ? "  " + Fmt.watts(f.power.batteries[0].watts, loc) : ""),
                series: "power.watts",
                max: 0,
                color: Theme.pulse.power
            });
        if (f.sensors.length > 0)
            d.push({
                id: "sensors",
                icon: "pulse_thermo",
                name: I18n.tr("Sensors"),
                sub: f.cpu.temp ? Fmt.celsius(f.cpu.temp, loc) : f.sensors.length + "",
                series: "cpu.temp",
                max: 0,
                color: Theme.pulse.sensor
            });
        return d;
    }
    readonly property var dev: devices.find(d => d.id === PulseUi.device) ?? devices[0] ?? null
    readonly property string kind: (dev?.id ?? "").split(":")[0]
    readonly property string arg: (dev?.id ?? "").split(":").slice(1).join(":")
    readonly property var gpu: kind === "gpu" ? f?.gpus.find(g => g.card === arg) ?? null : null
    readonly property var disk: kind === "disk" ? f?.disks.find(x => x.name === arg) ?? null : null
    readonly property var net: kind === "net" ? f?.net.find(x => x.iface === arg) ?? null : null
    readonly property var bat: f?.power.batteries[0] ?? null
    // Every temperature and fan with its own colour, in a stable order.
    readonly property var readings: {
        const t = (f?.sensors ?? []).map(x => ({
                    key: "sensor." + x.chip + "/" + x.label,
                    label: x.label,
                    chip: x.chip,
                    value: x.celsius,
                    crit: x.crit,
                    fan: false
                }));
        const n = Math.max(1, t.length);
        t.forEach((x, i) => x.color = Qt.hsla((0.02 + i / n * 0.85) % 1, 0.62, Theme.pulse.light ? 0.45 : 0.66, 1));
        const fans = (f?.fans ?? []).map(x => ({
                    key: "fan." + x.chip + "/" + x.label,
                    label: x.label,
                    chip: x.chip,
                    value: x.rpm,
                    fan: true,
                    color: Theme.colors.textMuted
                }));
        return t.concat(fans);
    }

    component Stat: ColumnLayout {
        property string label
        property string value
        property color tint: Theme.colors.text

        spacing: 0

        StyledText {
            text: parent.label
            color: Theme.colors.textMuted
            font.pixelSize: Theme.font.small
        }

        StyledText {
            Layout.fillWidth: true
            text: parent.value
            color: parent.tint
            font.pixelSize: Theme.font.title
            font.weight: Theme.font.weightMedium
            font.features: { "tnum": 1 }
        }
    }

    component BigGraph: ColumnLayout {
        property alias values: g.values
        property alias values2: g.values2
        property alias color: g.color
        property alias color2: g.color2
        property alias max: g.max
        property alias minMax: g.minMax
        property alias format: g.format
        property alias label1: g.label1
        property alias label2: g.label2
        property alias values3: g.values3
        property alias label3: g.label3
        property alias color3: g.color3
        property alias format3: g.format3
        property alias max3: g.max3
        property string title
        property string rightText

        spacing: Theme.spacing.xs

        RowLayout {
            Layout.fillWidth: true

            StyledText {
                Layout.fillWidth: true
                text: parent.parent.title
                color: Theme.colors.textMuted
                font.pixelSize: Theme.font.small
            }

            StyledText {
                text: parent.parent.rightText
                color: Theme.colors.textMuted
                font.pixelSize: Theme.font.small
            }
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.fillHeight: true
            radius: Theme.radius.small
            color: Theme.withAlpha(g.color, 0.04)
            border.width: 1
            border.color: Theme.withAlpha(g.color, 0.28)

            Graph {
                id: g

                anchors.fill: parent
                anchors.margins: 1
                points: page.pts
                live: !Pulse.longRange
                stepMs: Pulse.longRange ? Pulse.longStep() : Pulse.interval
            }
        }
    }

    RowLayout {
        anchors.fill: parent
        spacing: 0

        // Device list
        ListView {
            id: devList
            ScrollBar {
                parent: devList
                flick: devList
            }


            Layout.preferredWidth: 280
            Layout.fillHeight: true
            Layout.margins: Theme.spacing.md
            // A count: rows (and their graphs) stay while the data updates.
            model: page.devices.length
            spacing: 4
            clip: true
            readonly property int selected: Math.max(0, page.devices.findIndex(d => d.id === (page.dev?.id ?? "")))

            // The selection glides between devices.
            Rectangle {
                y: devList.selected * (66 + devList.spacing)
                width: devList.width
                height: 66
                radius: Theme.radius.small
                color: Theme.colors.selected
                border.width: 1
                border.color: Theme.colors.selectedRing
                z: -1
                Behavior on y {
                    SpringAnim {
                        duration: Theme.anim.normal
                    }
                }
            }

            delegate: Item {
                id: devItem

                required property int index
                readonly property var modelData: page.devices[index] ?? ({ id: "", icon: "", name: "", sub: "", series: "", max: 0, color: "transparent" })

                width: ListView.view.width
                height: 66

                Clickable {
                    radius: Theme.radius.small
                    onClicked: PulseUi.device = devItem.modelData.id
                }

                RowLayout {
                    anchors.fill: parent
                    anchors.margins: Theme.spacing.sm
                    spacing: Theme.spacing.sm

                    Rectangle {
                        Layout.preferredWidth: 78
                        Layout.fillHeight: true
                        radius: 6
                        color: Theme.withAlpha(devItem.modelData.color, 0.06)
                        border.width: 1
                        border.color: Theme.withAlpha(devItem.modelData.color, 0.35)
                        clip: true

                        Graph {
                            anchors.fill: parent
                            anchors.margins: 1
                            values: Pulse.series(devItem.modelData.series, 62)
                            max: devItem.modelData.max
                            points: 60
                            grid: false
                            hoverable: false
                            lineWidth: 1.5
                            color: devItem.modelData.color
                        }
                    }

                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 1

                        RowLayout {
                            spacing: Theme.spacing.xs

                            MaterialIcon {
                                icon: devItem.modelData.icon
                                size: Theme.icon.small
                                color: devItem.modelData.color
                            }

                            StyledText {
                                Layout.fillWidth: true
                                text: devItem.modelData.name
                                font.weight: Theme.font.weightMedium
                            }
                        }

                        StyledText {
                            Layout.fillWidth: true
                            text: devItem.modelData.sub
                            color: Theme.colors.textMuted
                            font.pixelSize: Theme.font.small
                            font.features: { "tnum": 1 }
                        }
                    }
                }
            }
        }

        Rectangle {
            Layout.fillHeight: true
            width: 1
            color: Theme.colors.divider
        }

        // Details
        Item {
            id: detail

            Layout.fillWidth: true
            Layout.fillHeight: true

            // Crossfade when switching devices.
            property string shownId: page.dev?.id ?? ""
            onShownIdChanged: fade.restart()

            SequentialAnimation {
                id: fade

                Anim {
                    target: detailCol
                    property: "opacity"
                    from: 0.35
                    to: 1
                    duration: Theme.anim.normal
                }
            }

            ColumnLayout {
                id: detailCol

                anchors.fill: parent
                anchors.margins: Theme.spacing.xl
                spacing: Theme.spacing.lg

                // Title row
                RowLayout {
                    Layout.fillWidth: true
                    spacing: Theme.spacing.md

                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 0

                        StyledText {
                            text: page.dev?.name ?? ""
                            font.pixelSize: Theme.font.large + 4
                            font.weight: Theme.font.weightSemiBold
                        }

                        StyledText {
                            Layout.fillWidth: true
                            color: Theme.colors.textMuted
                            text: {
                                switch (page.kind) {
                                case "cpu":
                                    return page.f?.cpu.model ?? "";
                                case "memory":
                                    return I18n.tr("%1 installed", Fmt.bytes(page.f?.memory.total ?? 0, page.loc));
                                case "gpu":
                                    return (page.gpu?.vendor ?? "") + " · " + (page.gpu?.driver ?? "") + " · " + (page.gpu?.pdev ?? "");
                                case "disk":
                                    return (page.disk?.model || page.disk?.name || "") + " · " + Fmt.bytes(page.disk?.size ?? 0, page.loc);
                                case "net":
                                    return page.net?.iface ?? "";
                                case "power":
                                    return page.f?.power.onAc ? I18n.tr("Plugged in") : I18n.tr("On battery");
                                }
                                return "";
                            }
                        }
                    }

                    Segmented {
                        visible: page.kind === "sensors"
                        options: [
                            {
                                k: "list",
                                t: I18n.tr("List"),
                                i: "list"
                            },
                            {
                                k: "graphs",
                                t: I18n.tr("Graphs"),
                                i: "grid"
                            },
                            {
                                k: "chart",
                                t: I18n.tr("One chart"),
                                i: "pulse_heart"
                            }
                        ]
                        current: Pulse.sensorView
                        onPicked: k => Pulse.setSensorView(k)
                    }

                    PillButton {
                        visible: page.kind === "cpu" && page.f?.cpu.temp != null
                        icon: "pulse_thermo"
                        checkable: true
                        checked: Pulse.cpuTemp
                        text: I18n.tr("Temperature")
                        onClicked: Pulse.setCpuTemp(!Pulse.cpuTemp)
                    }

                    PillButton {
                        visible: page.kind === "cpu"
                        icon: "grid"
                        text: page.perCore ? I18n.tr("Overall") : I18n.tr("Per core")
                        onClicked: page.perCore = !page.perCore
                    }

                    // History range; beyond 5 minutes from the daemon's recording.
                    Segmented {
                        options: [
                            {
                                k: "60",
                                t: I18n.tr("1 min")
                            },
                            {
                                k: "300",
                                t: I18n.tr("5 min")
                            },
                            {
                                k: "3600",
                                t: I18n.tr("1 h")
                            },
                            {
                                k: "86400",
                                t: I18n.tr("24 h")
                            },
                            {
                                k: "604800",
                                t: I18n.tr("7 d")
                            }
                        ]
                        current: String(Pulse.range)
                        onPicked: k => Pulse.setRange(Number(k))
                    }
                }

                // Graphs
                Item {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    Layout.minimumHeight: 220

                    StyledText {
                        anchors.centerIn: parent
                        z: 2
                        visible: Pulse.longRange && !VelaConfig.pulse.record
                        text: I18n.tr("Background recording is off (Settings → Pulse).")
                        color: Theme.colors.textMuted
                    }

                    // CPU overall
                    BigGraph {
                        anchors.fill: parent
                        visible: page.kind === "cpu" && !page.perCore
                        title: I18n.tr("Usage over %1", ({ 60: I18n.tr("60 seconds"), 300: I18n.tr("5 minutes"), 3600: I18n.tr("1 hour"), 86400: I18n.tr("24 hours"), 604800: I18n.tr("7 days") })[Pulse.range] ?? "")
                        rightText: page.showTemp ? "100 % · " + Fmt.celsius(page.tempTop, page.loc) : "100 %"
                        values: page.ser("cpu")
                        values2: page.ser("cpu.iowait")
                        label2: I18n.tr("I/O wait")
                        values3: page.showTemp ? page.ser("cpu.temp") : []
                        label3: I18n.tr("Temperature")
                        color3: Theme.pulse.heat
                        max3: page.tempTop
                        format3: v => Fmt.celsius(v, page.loc)
                        color: Theme.pulse.cpu
                        color2: Theme.pulse.disk
                        max: 100
                        format: v => Fmt.percent(v, page.loc)
                    }

                    // CPU per core
                    GridLayout {
                        anchors.fill: parent
                        visible: page.kind === "cpu" && page.perCore
                        readonly property int n: page.f?.cpu.cores.length ?? 1
                        columns: Math.ceil(Math.sqrt(n * 2))
                        columnSpacing: Theme.spacing.xs
                        rowSpacing: Theme.spacing.xs

                        Repeater {
                            model: page.kind === "cpu" && page.perCore ? (page.f?.cpu.cores.length ?? 0) : 0

                            Rectangle {
                                required property int index
                                // Slowed down by heat right now: the tile turns red.
                                readonly property bool throttled: VelaConfig.pulse.throttleTint && (page.f?.cpu.coreThrottled?.[index] ?? false)
                                readonly property color tint: throttled ? Theme.pulse.crit : Theme.pulse.cpu

                                Layout.fillWidth: true
                                Layout.fillHeight: true
                                radius: 6
                                color: Theme.withAlpha(tint, throttled ? 0.14 : 0.04)
                                border.width: 1
                                border.color: Theme.withAlpha(tint, throttled ? 0.55 : 0.28)
                                Behavior on color {
                                    ColorAnim {}
                                }
                                Behavior on border.color {
                                    ColorAnim {}
                                }

                                Graph {
                                    anchors.fill: parent
                                    anchors.margins: 1
                                    values: Pulse.longRange ? Pulse.longSeries("core." + parent.index) : Pulse.series("core." + parent.index, Math.min(page.pts, 120) + 2)
                                    points: Pulse.longRange ? page.pts : Math.min(page.pts, 120)
                                    live: !Pulse.longRange
                                    stepMs: Pulse.longRange ? Pulse.longStep() : Pulse.interval
                                    max: 100
                                    grid: false
                                    lineWidth: 1.5
                                    color: parent.tint
                                    format: v => Fmt.percent(v, page.loc)
                                    // Its core's sensor, else the package's.
                                    readonly property bool ownTemp: (page.f?.cpu.coreTemps?.length ?? 0) > parent.index
                                    values3: !page.showTemp ? [] : ownTemp ? (Pulse.longRange ? Pulse.longSeries("core.temp." + parent.index) : Pulse.series("core.temp." + parent.index, Math.min(page.pts, 120) + 2)) : page.ser("cpu.temp")
                                    label3: I18n.tr("Temperature")
                                    color3: Theme.pulse.heat
                                    max3: page.tempTop
                                    format3: v => Fmt.celsius(v, page.loc)
                                }

                                StyledText {
                                    x: 6
                                    y: 4
                                    readonly property var temp: page.f?.cpu.coreTemps?.[parent.index] ?? page.f?.cpu.temp ?? null
                                    text: I18n.tr("CPU %1", parent.index) + "  " + Fmt.mhz(page.f?.cpu.mhz[parent.index] ?? 0, page.loc) + (page.showTemp && temp !== null ? "  " + Fmt.celsius(temp, page.loc) : "") + (parent.throttled ? "  " + I18n.tr("throttled") : "")
                                    color: parent.throttled ? Theme.pulse.crit : Theme.colors.textMuted
                                    font.pixelSize: Theme.font.small - 1
                                }
                            }
                        }
                    }

                    ColumnLayout {
                        anchors.fill: parent
                        visible: page.kind === "memory"
                        spacing: Theme.spacing.md

                        BigGraph {
                            Layout.fillWidth: true
                            Layout.fillHeight: true
                            Layout.preferredHeight: 3
                            title: I18n.tr("Memory in use")
                            rightText: Fmt.bytes(page.f?.memory.total ?? 0, page.loc)
                            values: page.ser("mem")
                            color: Theme.pulse.memory
                            max: 100
                            format: v => Fmt.percent(v, page.loc)
                        }

                        BigGraph {
                            Layout.fillWidth: true
                            Layout.fillHeight: true
                            Layout.preferredHeight: 1
                            visible: (page.f?.memory.swapTotal ?? 0) > 0
                            title: I18n.tr("Swap")
                            rightText: Fmt.bytes(page.f?.memory.swapTotal ?? 0, page.loc)
                            values: page.ser("swap")
                            color: Theme.withAlpha(Theme.pulse.memory, 0.7)
                            max: 100
                            format: v => Fmt.percent(v, page.loc)
                        }

                        // Composition: in use, cache, free.
                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: Theme.spacing.xs
                            readonly property var m: page.f?.memory ?? null

                            Meter {
                                Layout.fillWidth: true
                                implicitHeight: 14
                                max: parent.m?.total ?? 1
                                segments: parent.m ? [
                                    {
                                        value: parent.m.used,
                                        color: Theme.pulse.memory
                                    },
                                    {
                                        value: parent.m.cache,
                                        color: Theme.withAlpha(Theme.pulse.memory, 0.4)
                                    }
                                ] : []
                            }

                            RowLayout {
                                spacing: Theme.spacing.lg

                                Repeater {
                                    model: [
                                        [I18n.tr("In use"), Theme.pulse.memory],
                                        [I18n.tr("Cache (frees itself)"), Theme.withAlpha(Theme.pulse.memory, 0.4)],
                                        [I18n.tr("Free"), Theme.withAlpha(Theme.colors.text, 0.12)]
                                    ]

                                    RowLayout {
                                        required property var modelData

                                        spacing: Theme.spacing.xs

                                        Rectangle {
                                            implicitWidth: 10
                                            implicitHeight: 10
                                            radius: 3
                                            color: parent.modelData[1]
                                        }

                                        StyledText {
                                            text: parent.modelData[0]
                                            color: Theme.colors.textMuted
                                            font.pixelSize: Theme.font.small
                                        }
                                    }
                                }
                            }
                        }
                    }

                    ColumnLayout {
                        anchors.fill: parent
                        visible: page.kind === "gpu"
                        spacing: Theme.spacing.md

                        BigGraph {
                            Layout.fillWidth: true
                            Layout.fillHeight: true
                            Layout.preferredHeight: 3
                            title: page.gpu?.asleep ? I18n.tr("Asleep — saves power") : I18n.tr("Usage")
                            rightText: "100 %"
                            values: page.ser("gpu." + page.arg)
                            color: Theme.pulse.gpu
                            max: 100
                            format: v => Fmt.percent(v, page.loc)
                        }

                        BigGraph {
                            Layout.fillWidth: true
                            Layout.fillHeight: true
                            Layout.preferredHeight: 1
                            visible: (page.gpu?.vramTotal ?? 0) > 0
                            title: I18n.tr("Video memory")
                            rightText: Fmt.bytes(page.gpu?.vramTotal ?? 0, page.loc)
                            values: page.ser("vram." + page.arg)
                            color: Theme.withAlpha(Theme.pulse.gpu, 0.7)
                            max: 100
                            format: v => Fmt.percent(v, page.loc)
                        }
                    }

                    ColumnLayout {
                        anchors.fill: parent
                        visible: page.kind === "disk"
                        spacing: Theme.spacing.md

                        BigGraph {
                            Layout.fillWidth: true
                            Layout.fillHeight: true
                            Layout.preferredHeight: 2
                            title: I18n.tr("Busy time")
                            rightText: "100 %"
                            values: page.ser("disk.busy." + page.arg)
                            color: Theme.pulse.disk
                            max: 100
                            format: v => Fmt.percent(v, page.loc)
                        }

                        BigGraph {
                            Layout.fillWidth: true
                            Layout.fillHeight: true
                            Layout.preferredHeight: 2
                            title: I18n.tr("Transfer rate")
                            values: page.ser("disk.r." + page.arg)
                            values2: page.ser("disk.w." + page.arg)
                            label1: I18n.tr("Read")
                            label2: I18n.tr("Write")
                            color: Theme.pulse.disk
                            color2: Theme.pulse.diskWrite
                            minMax: 1024 * 1024
                            format: v => Fmt.rate(v, page.loc)
                        }
                    }

                    BigGraph {
                        anchors.fill: parent
                        visible: page.kind === "net"
                        title: I18n.tr("Throughput")
                        values: page.ser("net.rx." + page.arg)
                        values2: page.ser("net.tx." + page.arg)
                        label1: I18n.tr("Down")
                        label2: I18n.tr("Up")
                        color: Theme.pulse.net
                        color2: Theme.pulse.netUp
                        minMax: 128 * 1024
                        format: v => Fmt.bits(v, page.loc)
                    }

                    BigGraph {
                        anchors.fill: parent
                        visible: page.kind === "power"
                        title: I18n.tr("Power draw from the battery")
                        values: page.ser("power.watts")
                        color: Theme.pulse.power
                        minMax: 10
                        format: v => Fmt.watts(v, page.loc)
                    }

                    // Sensors: every temperature, then fans.
                    Flickable {
                        id: sensorFlick

                        ScrollBar {
                            parent: sensorFlick
                            flick: sensorFlick
                        }

                        anchors.fill: parent
                        visible: page.kind === "sensors" && Pulse.sensorView === "list"
                        contentHeight: sensorGrid.implicitHeight
                        clip: true

                        GridLayout {
                            id: sensorGrid

                            width: parent.width
                            columns: width > 700 ? 2 : 1
                            columnSpacing: Theme.spacing.xl
                            rowSpacing: Theme.spacing.md

                            Repeater {
                                model: page.f?.sensors.length ?? 0

                                ColumnLayout {
                                    required property int index
                                    readonly property var modelData: page.f?.sensors[index] ?? ({ label: "", chip: "", celsius: 0 })

                                    Layout.fillWidth: true
                                    spacing: 3

                                    RowLayout {
                                        Layout.fillWidth: true

                                        StyledText {
                                            Layout.fillWidth: true
                                            text: parent.parent.modelData.label + "  ·  " + parent.parent.modelData.chip
                                        }

                                        Counter {
                                            value: parent.parent.modelData.celsius
                                            format: v => Fmt.celsius(v, page.loc)
                                            font.weight: Theme.font.weightMedium
                                            color: parent.parent.modelData.celsius >= (parent.parent.modelData.crit ?? 100) - 10 ? Theme.pulse.crit : Theme.colors.text
                                        }
                                    }

                                    Meter {
                                        Layout.fillWidth: true
                                        max: parent.modelData.crit ?? 100
                                        value: parent.modelData.celsius
                                        color: parent.modelData.celsius >= (parent.modelData.crit ?? 100) - 10 ? Theme.pulse.crit : Theme.pulse.sensor
                                    }
                                }
                            }

                            Repeater {
                                model: page.f?.fans.length ?? 0

                                RowLayout {
                                    required property int index
                                    readonly property var modelData: page.f?.fans[index] ?? ({ label: "", chip: "", rpm: 0 })

                                    Layout.fillWidth: true

                                    MaterialIcon {
                                        icon: "pulse_fan"
                                        color: Theme.pulse.sensor
                                    }

                                    StyledText {
                                        Layout.fillWidth: true
                                        text: parent.modelData.label + "  ·  " + parent.modelData.chip
                                    }

                                    Counter {
                                        value: parent.modelData.rpm
                                        format: v => I18n.tr("%1 rpm", Math.round(v))
                                        font.weight: Theme.font.weightMedium
                                    }
                                }
                            }
                        }
                    }

                    // Sensors, a graph each.
                    Flickable {
                        id: tilesFlick

                        ScrollBar {
                            parent: tilesFlick
                            flick: tilesFlick
                        }

                        anchors.fill: parent
                        visible: page.kind === "sensors" && Pulse.sensorView === "graphs"
                        contentHeight: tilesGrid.implicitHeight
                        clip: true
                        boundsBehavior: Flickable.StopAtBounds

                        GridLayout {
                            id: tilesGrid

                            width: parent.width
                            columns: Math.max(1, Math.floor(width / 300))
                            columnSpacing: Theme.spacing.md
                            rowSpacing: Theme.spacing.md

                            Repeater {
                                model: page.kind === "sensors" && Pulse.sensorView === "graphs" ? page.readings.length : 0

                                Rectangle {
                                    id: tile

                                    required property int index
                                    readonly property var r: page.readings[index] ?? ({ key: "", label: "", value: 0, fan: false })
                                    readonly property bool hot: !r.fan && r.value >= (r.crit ?? 100) - 10

                                    Layout.fillWidth: true
                                    Layout.preferredHeight: 132
                                    radius: Theme.radius.small
                                    color: Theme.withAlpha(r.color, 0.04)
                                    border.width: 1
                                    border.color: Theme.withAlpha(tile.hot ? Theme.pulse.crit : r.color, 0.3)

                                    ColumnLayout {
                                        anchors.fill: parent
                                        anchors.margins: Theme.spacing.sm
                                        spacing: 2

                                        RowLayout {
                                            Layout.fillWidth: true

                                            MaterialIcon {
                                                icon: tile.r.fan ? "pulse_fan" : "pulse_thermo"
                                                size: Theme.icon.small
                                                color: tile.r.color
                                            }

                                            StyledText {
                                                Layout.fillWidth: true
                                                text: tile.r.label
                                                font.pixelSize: Theme.font.small + 1
                                            }

                                            Counter {
                                                value: tile.r.value
                                                format: v => tile.r.fan ? I18n.tr("%1 rpm", Math.round(v)) : Fmt.celsius(v, page.loc)
                                                font.weight: Theme.font.weightMedium
                                                color: tile.hot ? Theme.pulse.crit : Theme.colors.text
                                            }
                                        }

                                        StyledText {
                                            text: tile.r.chip
                                            color: Theme.colors.textMuted
                                            font.pixelSize: Theme.font.small - 1
                                        }

                                        Graph {
                                            Layout.fillWidth: true
                                            Layout.fillHeight: true
                                            values: page.ser(tile.r.key)
                                            points: page.pts
                                            live: !Pulse.longRange
                                            stepMs: Pulse.longRange ? Pulse.longStep() : Pulse.interval
                                            minMax: tile.r.fan ? 1000 : 50
                                            grid: false
                                            lineWidth: 1.5
                                            color: tile.hot ? Theme.pulse.crit : tile.r.color
                                            format: v => tile.r.fan ? I18n.tr("%1 rpm", Math.round(v)) : Fmt.celsius(v, page.loc)
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // All temperatures in one chart; the legend highlights one.
                    ColumnLayout {
                        id: chart

                        property string focusKey: ""
                        readonly property var temps: page.readings.filter(r => !r.fan)
                        readonly property real chartTop: Math.max(50, Fmt.niceMax(temps.map(t => Math.max(t.value, ...page.ser(t.key))), 50))

                        anchors.fill: parent
                        visible: page.kind === "sensors" && Pulse.sensorView === "chart"
                        spacing: Theme.spacing.md

                        Rectangle {
                            Layout.fillWidth: true
                            Layout.fillHeight: true
                            radius: Theme.radius.small
                            color: Theme.withAlpha(Theme.pulse.sensor, 0.03)
                            border.width: 1
                            border.color: Theme.withAlpha(Theme.pulse.sensor, 0.25)

                            Repeater {
                                model: chart.visible ? chart.temps.length : 0

                                Graph {
                                    required property int index
                                    readonly property var r: chart.temps[index] ?? ({ key: "", color: "transparent" })

                                    anchors.fill: parent
                                    anchors.margins: 1
                                    values: page.ser(r.key)
                                    points: page.pts
                                    live: !Pulse.longRange
                                    stepMs: Pulse.longRange ? Pulse.longStep() : Pulse.interval
                                    max: chart.chartTop
                                    fill: false
                                    grid: index === 0
                                    hoverable: false
                                    lineWidth: chart.focusKey === r.key ? 3 : 1.8
                                    color: r.color
                                    opacity: chart.focusKey === "" || chart.focusKey === r.key ? 1 : 0.18
                                    Behavior on opacity {
                                        Anim {}
                                    }
                                }
                            }

                            StyledText {
                                anchors.right: parent.right
                                anchors.top: parent.top
                                anchors.margins: Theme.spacing.sm
                                text: Fmt.celsius(chart.chartTop, page.loc)
                                color: Theme.colors.textMuted
                                font.pixelSize: Theme.font.small
                            }
                        }

                        Flow {
                            Layout.fillWidth: true
                            spacing: Theme.spacing.sm

                            Repeater {
                                model: chart.temps.length

                                Rectangle {
                                    required property int index
                                    readonly property var r: chart.temps[index] ?? ({ key: "", label: "", value: 0, color: "transparent" })

                                    implicitHeight: 28
                                    implicitWidth: legendRow.implicitWidth + 2 * Theme.spacing.sm
                                    radius: 14
                                    color: chart.focusKey === r.key ? Theme.colors.selected : Theme.colors.chip
                                    Behavior on color {
                                        ColorAnim {
                                            duration: Theme.anim.fast
                                        }
                                    }

                                    MouseArea {
                                        anchors.fill: parent
                                        hoverEnabled: true
                                        onEntered: chart.focusKey = parent.r.key
                                        onExited: if (chart.focusKey === parent.r.key)
                                            chart.focusKey = ""
                                    }

                                    RowLayout {
                                        id: legendRow

                                        anchors.centerIn: parent
                                        spacing: Theme.spacing.xs

                                        Rectangle {
                                            implicitWidth: 10
                                            implicitHeight: 10
                                            radius: 5
                                            color: parent.parent.r.color
                                        }

                                        StyledText {
                                            text: parent.parent.r.label
                                            font.pixelSize: Theme.font.small
                                        }

                                        Counter {
                                            value: parent.parent.r.value
                                            format: v => Fmt.celsius(v, page.loc)
                                            color: Theme.colors.textMuted
                                            font.pixelSize: Theme.font.small
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // Numbers
                GridLayout {
                    Layout.fillWidth: true
                    visible: page.kind !== "sensors"
                    columns: Math.max(3, Math.floor(width / 170))
                    columnSpacing: Theme.spacing.xl
                    rowSpacing: Theme.spacing.md

                    Repeater {
                        model: {
                            const f = page.f;
                            if (!f)
                                return [];
                            const L = page.loc;
                            const rows = [];
                            const add = (l, v, tint) => rows.push({
                                    l: l,
                                    v: v,
                                    tint: tint
                                });
                            switch (page.kind) {
                            case "cpu":
                                {
                                    const c = f.cpu;
                                    add(I18n.tr("Usage"), Fmt.percent(c.usage, L));
                                    add(I18n.tr("Speed"), Fmt.mhz(c.avgMhz, L));
                                    if (c.maxMhz)
                                        add(I18n.tr("Max speed"), Fmt.mhz(c.maxMhz, L));
                                    if (c.temp !== null && c.temp !== undefined)
                                        add(I18n.tr("Temperature"), Fmt.celsius(c.temp, L), c.temp >= (c.tempCrit ?? 100) - 8 ? Theme.pulse.crit : undefined);
                                    add(I18n.tr("Processes"), String(c.processes));
                                    add(I18n.tr("Threads"), String(c.threads));
                                    add(I18n.tr("Load (1/5/15 min)"), c.load.map(v => Fmt.num(v, 2, L)).join(" "));
                                    add(I18n.tr("Waiting for CPU"), Fmt.percent(c.pressure, L), c.pressure >= 20 ? Theme.pulse.warn : undefined);
                                    add(I18n.tr("Context switches"), I18n.tr("%1/s", Fmt.num(c.ctxtPerSec, 0, L)));
                                    add(I18n.tr("Logical processors"), String(c.logical));
                                    if (c.governor)
                                        add(I18n.tr("Governor"), c.governor + (c.epp ? " · " + c.epp : ""));
                                    if (c.throttleTotal)
                                        add(I18n.tr("Throttled since boot"), I18n.tr("%1 times", Fmt.num(c.throttleTotal, 0, L)), c.throttled > 0 ? Theme.pulse.warn : undefined);
                                    add(I18n.tr("Up time"), Fmt.clock(c.uptime));
                                    if (c.cache)
                                        add(I18n.tr("Cache"), c.cache);
                                    break;
                                }
                            case "memory":
                                {
                                    const m = f.memory;
                                    add(I18n.tr("In use"), Fmt.bytes(m.used, L));
                                    add(I18n.tr("Available"), Fmt.bytes(m.available, L));
                                    add(I18n.tr("Cache"), Fmt.bytes(m.cache, L));
                                    add(I18n.tr("Free"), Fmt.bytes(m.free, L));
                                    add(I18n.tr("Shared"), Fmt.bytes(m.shmem, L));
                                    add(I18n.tr("Waiting to be written"), Fmt.bytes(m.dirty, L));
                                    if (m.swapTotal > 0)
                                        add(I18n.tr("Swap"), Fmt.bytes(m.swapUsed, L) + " / " + Fmt.bytes(m.swapTotal, L));
                                    if (m.zswapped > 0)
                                        add(I18n.tr("Compressed (zswap)"), Fmt.bytes(m.zswapped, L) + " → " + Fmt.bytes(m.zswap, L));
                                    add(I18n.tr("Swap in / out"), Fmt.rate(m.swapInBps, L) + " / " + Fmt.rate(m.swapOutBps, L), m.swapInBps + m.swapOutBps > 1048576 ? Theme.pulse.warn : undefined);
                                    add(I18n.tr("Page faults"), I18n.tr("%1/s", Fmt.num(m.majorFaultsPerSec, 0, L)));
                                    add(I18n.tr("Waiting for memory"), Fmt.percent(m.pressure, L), m.pressure >= 10 ? Theme.pulse.warn : undefined);
                                    add(I18n.tr("Out-of-memory kills"), String(m.oomKills), m.oomKills > 0 ? Theme.pulse.warn : undefined);
                                    break;
                                }
                            case "gpu":
                                {
                                    const g = page.gpu;
                                    if (!g)
                                        break;
                                    add(I18n.tr("State"), g.asleep ? I18n.tr("Asleep") : I18n.tr("Awake"), g.asleep ? Theme.pulse.ok : undefined);
                                    add(I18n.tr("Usage"), Fmt.percent(g.busy ?? 0, L));
                                    if (g.vramTotal)
                                        add(I18n.tr("Video memory"), Fmt.bytes(g.vramUsed, L) + " / " + Fmt.bytes(g.vramTotal, L));
                                    if (g.mhz)
                                        add(I18n.tr("Speed"), Fmt.mhz(g.mhz, L) + (g.maxMhz ? " / " + Fmt.mhz(g.maxMhz, L) : ""));
                                    if (g.temp)
                                        add(I18n.tr("Temperature"), Fmt.celsius(g.temp, L));
                                    if (g.watts)
                                        add(I18n.tr("Power"), Fmt.watts(g.watts, L));
                                    if (g.encoder !== null && g.encoder !== undefined)
                                        add(I18n.tr("Video encoder"), Fmt.percent(g.encoder, L));
                                    if (g.decoder !== null && g.decoder !== undefined)
                                        add(I18n.tr("Video decoder"), Fmt.percent(g.decoder, L));
                                    add(I18n.tr("Driver"), g.driver);
                                    const users = Pulse.apps.filter(a => a.gpus.indexOf(g.card) >= 0 && a.kind !== "kernel").map(a => a.name);
                                    add(I18n.tr("Used by"), users.length > 0 ? Words.names(users) : I18n.tr("nothing"));
                                    break;
                                }
                            case "disk":
                                {
                                    const d = page.disk;
                                    if (!d)
                                        break;
                                    add(I18n.tr("Read"), Fmt.rate(d.readBps, L));
                                    add(I18n.tr("Write"), Fmt.rate(d.writeBps, L));
                                    add(I18n.tr("Busy"), Fmt.percent(d.busy, L));
                                    add(I18n.tr("Type"), d.removable ? I18n.tr("Removable") : d.rotational ? I18n.tr("Hard disk") : I18n.tr("SSD"));
                                    add(I18n.tr("Capacity"), Fmt.bytes(d.size, L));
                                    add(I18n.tr("Waiting for I/O"), Fmt.percent(f.io.pressureFull, L), f.io.pressureFull >= 10 ? Theme.pulse.warn : undefined);
                                    for (const m of d.mounts)
                                        add(m.path + " (" + m.fstype + ")", I18n.tr("%1 free of %2", Fmt.bytes(m.total - m.used, L), Fmt.bytes(m.total, L)), m.used / m.total > 0.9 ? Theme.pulse.crit : undefined);
                                    break;
                                }
                            case "net":
                                {
                                    const n = page.net;
                                    if (!n)
                                        break;
                                    add(I18n.tr("Down"), Fmt.bits(n.rxBps, L));
                                    add(I18n.tr("Up"), Fmt.bits(n.txBps, L));
                                    add(I18n.tr("Received"), Fmt.bytes(n.rxTotal, L));
                                    add(I18n.tr("Sent"), Fmt.bytes(n.txTotal, L));
                                    add(I18n.tr("State"), n.up ? I18n.tr("Connected") : I18n.tr("Disconnected"));
                                    if (n.speedMbps)
                                        add(I18n.tr("Link speed"), I18n.tr("%1 Mbit/s", n.speedMbps));
                                    break;
                                }
                            case "power":
                                {
                                    const b = page.bat;
                                    if (!b)
                                        break;
                                    add(I18n.tr("Charge"), Fmt.percent(b.percent, L, 0));
                                    add(I18n.tr("State"), b.status === "charging" ? I18n.tr("Charging") : b.status === "discharging" ? I18n.tr("Discharging") : b.status === "full" ? I18n.tr("Full") : I18n.tr("Not charging"));
                                    add(I18n.tr("Power"), Fmt.watts(b.watts, L));
                                    if (b.secondsLeft)
                                        add(b.status === "charging" ? I18n.tr("Full in") : I18n.tr("Time left"), I18n.duration(b.secondsLeft));
                                    add(I18n.tr("Energy"), Fmt.num(b.energyWh, 1, L) + " / " + Fmt.num(b.fullWh, 1, L) + " Wh");
                                    if (b.designWh > 0)
                                        add(I18n.tr("Health"), Fmt.percent(Math.min(100, b.fullWh / b.designWh * 100), L, 0), b.fullWh / b.designWh < 0.7 ? Theme.pulse.warn : undefined);
                                    if (b.cycles)
                                        add(I18n.tr("Charge cycles"), String(b.cycles));
                                    if (f.powerProfile)
                                        add(I18n.tr("Power profile"), Words.profile(f.powerProfile));
                                    break;
                                }
                            }
                            return rows;
                        }

                        Stat {
                            required property var modelData

                            Layout.fillWidth: true
                            label: modelData.l
                            value: modelData.v
                            tint: modelData.tint ?? Theme.colors.text
                        }
                    }
                }
            }
        }
    }
}
