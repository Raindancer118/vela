import QtQuick
import QtQuick.Layouts
import qs
import qs.components
import "Fmt.js" as Fmt

// Details of one app (PulseUi.detailKey) sliding in from the right: what's
// wrong with it and how to fix it, its load over time, facts, its
// processes, and everything about the selected process.
Item {
    id: root

    readonly property bool open: PulseUi.detailKey !== ""
    property string shownKey: ""
    readonly property var app: Pulse.app(shownKey)
    readonly property int pid: PulseUi.detailPid > 0 ? PulseUi.detailPid : (app?.mainPid ?? 0)
    readonly property var d: Pulse.details && Pulse.details.pid === pid ? Pulse.details : null
    readonly property var loc: Qt.locale()

    width: Theme.pulse.flyoutWidth
    visible: slide.x < width

    onOpenChanged: if (open)
        shownKey = PulseUi.detailKey
    Connections {
        target: PulseUi

        function onDetailKeyChanged(): void {
            if (PulseUi.detailKey !== "") {
                if (root.shownKey !== PulseUi.detailKey)
                    swap.restart();
                else
                    root.shownKey = PulseUi.detailKey;
            }
        }
    }
    onPidChanged: if (pid > 0)
        Pulse.loadDetails(pid)

    // Switching between apps: the content fades through.
    SequentialAnimation {
        id: swap

        Anim {
            target: content
            property: "opacity"
            to: 0
            duration: Theme.anim.fast
        }
        ScriptAction {
            script: {
                root.shownKey = PulseUi.detailKey;
                scroll.contentY = 0;
            }
        }
        Anim {
            target: content
            property: "opacity"
            to: 1
        }
    }

    Timer {
        running: root.open && root.pid > 0
        interval: 2000
        repeat: true
        onTriggered: Pulse.loadDetails(root.pid)
    }

    // The app went away while open: say so instead of closing.
    readonly property bool gone: open && shownKey !== "" && app === null

    Item {
        id: slide

        width: parent.width
        height: parent.height
        x: root.open ? 0 : root.width + 24
        Behavior on x {
            SpringAnim {
                duration: Theme.anim.slow
                easing.overshoot: 0.6
            }
        }

        Rectangle {
            anchors.fill: parent
            color: Theme.colors.background
            opacity: 0.97
        }

        Rectangle {
            width: 1
            height: parent.height
            color: Theme.colors.outline
        }

        // Shadow edge.
        Rectangle {
            anchors.right: parent.left
            width: 24
            height: parent.height
            gradient: Gradient {
                orientation: Gradient.Horizontal
                GradientStop {
                    position: 0
                    color: "transparent"
                }
                GradientStop {
                    position: 1
                    color: Qt.rgba(0, 0, 0, 0.18)
                }
            }
        }

        ColumnLayout {
            id: content

            anchors.fill: parent
            anchors.margins: Theme.spacing.lg
            spacing: Theme.spacing.md

            // Header
            RowLayout {
                Layout.fillWidth: true
                spacing: Theme.spacing.md

                AppIcon {
                    size: 44
                    icon: root.app?.icon ?? ""
                    name: root.app?.name ?? ""
                    fallback: root.app?.kind === "task" ? "utilities-terminal" : "application-x-executable"
                }

                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 2

                    StyledText {
                        Layout.fillWidth: true
                        text: root.app?.name ?? I18n.tr("Gone")
                        font.pixelSize: Theme.font.large
                        font.weight: Theme.font.weightSemiBold
                    }

                    RowLayout {
                        spacing: Theme.spacing.xs

                        Chip {
                            text: Words.kindOne(root.app?.kind ?? "")
                        }

                        Chip {
                            visible: root.app !== null
                            text: root.app?.health === 2 ? I18n.tr("Problem") : root.app?.health === 1 ? I18n.tr("Worth a look") : I18n.tr("Healthy")
                            tint: root.app?.health === 2 ? Theme.pulse.crit : root.app?.health === 1 ? Theme.pulse.warn : Theme.pulse.ok
                            icon: root.app?.health === 0 ? "check" : "warning"
                        }
                    }
                }

                IconButton {
                    icon: "close"
                    onClicked: PulseUi.detailKey = ""

                    Tip {
                        text: I18n.tr("Close")
                        shown: parent.hovered
                    }
                }
            }

            StyledText {
                visible: root.gone
                Layout.fillWidth: true
                text: I18n.tr("This app isn't running anymore.")
                color: Theme.colors.textMuted
            }

            Flickable {
                id: scroll

                Layout.fillWidth: true
                Layout.fillHeight: true
                contentHeight: inner.implicitHeight
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                visible: root.app !== null

                ColumnLayout {
                    id: inner

                    width: scroll.width
                    spacing: Theme.spacing.lg

                    // Flags with their explanation and the fix.
                    Repeater {
                        model: (root.app?.flags ?? []).filter(f => f !== "efficiency")

                        Rectangle {
                            required property string modelData

                            Layout.fillWidth: true
                            implicitHeight: flagCol.implicitHeight + 2 * Theme.spacing.md
                            radius: Theme.radius.card
                            color: Theme.withAlpha(Words.flagColor(modelData), 0.10)
                            border.width: 1
                            border.color: Theme.withAlpha(Words.flagColor(modelData), 0.3)

                            ColumnLayout {
                                id: flagCol

                                anchors.fill: parent
                                anchors.margins: Theme.spacing.md
                                spacing: Theme.spacing.xs

                                RowLayout {
                                    MaterialIcon {
                                        icon: Words.flagIcon(parent.parent.parent.modelData)
                                        color: Words.flagColor(parent.parent.parent.modelData)
                                    }

                                    StyledText {
                                        text: Words.flag(parent.parent.parent.modelData)
                                        font.weight: Theme.font.weightSemiBold
                                    }
                                }

                                StyledText {
                                    Layout.fillWidth: true
                                    text: {
                                        const f = parent.parent.modelData;
                                        if (f === "needs-restart" && (root.app?.restartReasons ?? []).length > 0)
                                            return Words.flagExplain(f) + "\n" + root.app.restartReasons.slice(0, 4).join("\n");
                                        if (f === "leak" && root.app?.leak)
                                            return I18n.tr("It grew by %1 per minute for %2 minutes. A restart frees it.", Fmt.bytes(root.app.leak.perMinute, root.loc), Math.round(root.app.leak.minutes));
                                        return Words.flagExplain(f);
                                    }
                                    color: Theme.colors.textMuted
                                    wrapMode: Text.Wrap
                                    font.pixelSize: Theme.font.small + 1
                                }

                                RowLayout {
                                    spacing: Theme.spacing.xs

                                    PillButton {
                                        readonly property string f: parent.parent.parent.modelData
                                        visible: ["not-responding", "hung"].indexOf(f) >= 0
                                        style: "danger"
                                        text: I18n.tr("Force quit")
                                        onClicked: PulseUi.endApp(root.app.key, true)
                                    }

                                    PillButton {
                                        readonly property string f: parent.parent.parent.modelData
                                        visible: ["not-responding", "needs-restart", "leak", "crashed", "zombies"].indexOf(f) >= 0 && root.app?.kind !== "system"
                                        text: I18n.tr("Restart")
                                        onClicked: PulseUi.restartApp(root.app.key)
                                    }

                                    PillButton {
                                        readonly property string f: parent.parent.parent.modelData
                                        visible: f === "runaway"
                                        text: I18n.tr("Efficiency mode")
                                        icon: "energy_savings_leaf"
                                        onClicked: Pulse.setEfficiency(root.app.key, true)
                                    }

                                    PillButton {
                                        readonly property string f: parent.parent.parent.modelData
                                        visible: f === "paused"
                                        text: I18n.tr("Resume")
                                        icon: "play_arrow"
                                        onClicked: Pulse.setPaused(root.app.key, false)
                                    }

                                    PillButton {
                                        readonly property string f: parent.parent.parent.modelData
                                        visible: f === "crashed" && VelaConfig.pulse.claude
                                        text: I18n.tr("Ask Claude")
                                        onClicked: Pulse.askClaude("crash:" + (root.app?.exe ?? ""))
                                    }
                                }
                            }
                        }
                    }

                    // CPU and memory over the last two minutes.
                    GridLayout {
                        Layout.fillWidth: true
                        columns: 2
                        columnSpacing: Theme.spacing.md
                        rowSpacing: Theme.spacing.xs

                        StyledText {
                            text: I18n.tr("CPU")
                            color: Theme.colors.textMuted
                            font.pixelSize: Theme.font.small
                        }

                        StyledText {
                            text: I18n.tr("Memory")
                            color: Theme.colors.textMuted
                            font.pixelSize: Theme.font.small
                        }

                        Counter {
                            value: root.app?.cpu ?? 0
                            format: v => Fmt.percent(v, root.loc)
                            font.pixelSize: Theme.font.title
                            font.weight: Theme.font.weightSemiBold
                        }

                        Counter {
                            value: root.app?.mem ?? 0
                            format: v => Fmt.bytes(v, root.loc)
                            font.pixelSize: Theme.font.title
                            font.weight: Theme.font.weightSemiBold
                        }

                        Graph {
                            Layout.fillWidth: true
                            Layout.preferredHeight: 70
                            values: Pulse.series("app.cpu." + root.shownKey, 122)
                            points: 120
                            minMax: 5
                            color: Theme.pulse.cpu
                            format: v => Fmt.percent(v, root.loc)
                        }

                        Graph {
                            Layout.fillWidth: true
                            Layout.preferredHeight: 70
                            values: Pulse.series("app.mem." + root.shownKey, 122)
                            points: 120
                            minMax: 64 * 1024 * 1024
                            color: Theme.pulse.memory
                            format: v => Fmt.bytes(v, root.loc)
                        }
                    }

                    // Facts
                    Repeater {
                        model: {
                            const a = root.app;
                            if (!a)
                                return [];
                            const rows = [[I18n.tr("Processes"), String(a.pids.length)], [I18n.tr("Threads"), String(a.threads)], [I18n.tr("Running for"), Words.since(a.started)], [I18n.tr("User"), a.user]];
                            if (a.readBps + a.writeBps > 0)
                                rows.push([I18n.tr("Disk"), Fmt.rate(a.readBps + a.writeBps, root.loc)]);
                            if (a.gpu > 0 || a.vram > 0)
                                rows.push([I18n.tr("GPU"), Fmt.percent(a.gpu, root.loc) + (a.vram > 0 ? "  ·  " + Fmt.bytes(a.vram, root.loc) : "")]);
                            if (a.gpus.length > 0)
                                rows.push([I18n.tr("Graphics cards"), a.gpus.map(c => (Pulse.frame?.gpus.find(g => g.card === c)?.name ?? c)).join(", ")]);
                            if (a.swap > 0)
                                rows.push([I18n.tr("In swap"), Fmt.bytes(a.swap, root.loc)]);
                            if (a.unit)
                                rows.push([I18n.tr("systemd unit"), a.unit]);
                            return rows;
                        }

                        RowLayout {
                            required property var modelData

                            Layout.fillWidth: true
                            spacing: Theme.spacing.md

                            StyledText {
                                Layout.preferredWidth: 130
                                text: parent.modelData[0]
                                color: Theme.colors.textMuted
                                font.pixelSize: Theme.font.small + 1
                            }

                            StyledText {
                                Layout.fillWidth: true
                                text: parent.modelData[1]
                                font.pixelSize: Theme.font.small + 1
                                wrapMode: Text.WrapAnywhere
                                maximumLineCount: 2
                            }
                        }
                    }

                    // Windows
                    ColumnLayout {
                        visible: (root.app?.windows ?? []).length > 0
                        Layout.fillWidth: true
                        spacing: 2

                        SectionLabel {
                            text: I18n.tr("Windows")
                        }

                        Repeater {
                            model: root.app?.windows ?? []

                            Item {
                                required property var modelData

                                Layout.fillWidth: true
                                implicitHeight: 34

                                Clickable {
                                    radius: Theme.radius.small
                                    onClicked: Pulse.focus(parent.modelData.address)
                                }

                                RowLayout {
                                    anchors.fill: parent
                                    anchors.leftMargin: Theme.spacing.sm
                                    anchors.rightMargin: Theme.spacing.sm

                                    MaterialIcon {
                                        icon: "focus"
                                        size: Theme.icon.small
                                        color: Theme.colors.textMuted
                                    }

                                    StyledText {
                                        Layout.fillWidth: true
                                        text: parent.parent.modelData.title || parent.parent.modelData["class"]
                                        font.pixelSize: Theme.font.small + 1
                                    }

                                    StyledText {
                                        text: parent.parent.modelData.workspace
                                        color: Theme.colors.textMuted
                                        font.pixelSize: Theme.font.small
                                    }
                                }
                            }
                        }
                    }

                    // Processes of the app
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 2

                        SectionLabel {
                            text: I18n.tr("Processes")
                        }

                        Repeater {
                            model: {
                                Pulse.revision;
                                return (root.app?.pids ?? []).map(p => Pulse.procMap[p]).filter(p => p).sort((a, b) => b.cpu - a.cpu).slice(0, 30);
                            }

                            Rectangle {
                                required property var modelData

                                Layout.fillWidth: true
                                implicitHeight: 32
                                radius: Theme.radius.small
                                color: modelData.pid === root.pid ? Theme.colors.selected : "transparent"
                                Behavior on color {
                                    ColorAnim {
                                        duration: Theme.anim.fast
                                    }
                                }

                                Clickable {
                                    radius: parent.radius
                                    acceptedButtons: Qt.LeftButton | Qt.RightButton
                                    onClicked: mouse => {
                                        PulseUi.detailPid = parent.modelData.pid;
                                        if (mouse.button === Qt.RightButton) {
                                            const p = mapToItem(null, mouse.x, mouse.y);
                                            PulseUi.openMenu(p.x, p.y, PulseUi.procMenu(parent.modelData));
                                        }
                                    }
                                }

                                RowLayout {
                                    anchors.fill: parent
                                    anchors.leftMargin: Theme.spacing.sm
                                    anchors.rightMargin: Theme.spacing.sm

                                    StyledText {
                                        Layout.fillWidth: true
                                        text: parent.parent.modelData.comm
                                        font.pixelSize: Theme.font.small + 1
                                    }

                                    StyledText {
                                        text: parent.parent.modelData.pid
                                        color: Theme.colors.textMuted
                                        font.pixelSize: Theme.font.small
                                        font.features: { "tnum": 1 }
                                    }

                                    StyledText {
                                        Layout.preferredWidth: 56
                                        horizontalAlignment: Text.AlignRight
                                        text: Fmt.percent(parent.parent.modelData.cpu / (Pulse.frame?.cpu.logical ?? 1), root.loc)
                                        font.pixelSize: Theme.font.small + 1
                                        font.features: { "tnum": 1 }
                                    }

                                    StyledText {
                                        Layout.preferredWidth: 70
                                        horizontalAlignment: Text.AlignRight
                                        text: Fmt.bytes(parent.parent.modelData.mem, root.loc)
                                        font.pixelSize: Theme.font.small + 1
                                        font.features: { "tnum": 1 }
                                    }
                                }
                            }
                        }
                    }

                    // The selected process in depth.
                    ColumnLayout {
                        visible: root.d !== null && !root.d.gone
                        Layout.fillWidth: true
                        spacing: Theme.spacing.xs

                        SectionLabel {
                            text: I18n.tr("Process %1", root.pid)
                        }

                        Rectangle {
                            Layout.fillWidth: true
                            implicitHeight: cmd.implicitHeight + 2 * Theme.spacing.sm
                            radius: Theme.radius.small
                            color: Theme.withAlpha(Theme.colors.text, 0.05)

                            TextEdit {
                                id: cmd

                                anchors.fill: parent
                                anchors.margins: Theme.spacing.sm
                                readOnly: true
                                selectByMouse: true
                                wrapMode: TextEdit.WrapAnywhere
                                color: Theme.colors.text
                                selectionColor: Theme.colors.primaryMuted
                                font.family: "monospace"
                                font.pixelSize: Theme.font.small
                                text: (root.d?.cmdline ?? []).join(" ")
                            }
                        }

                        Repeater {
                            model: {
                                const d = root.d;
                                if (!d || d.gone)
                                    return [];
                                const b = v => v === null || v === undefined ? "–" : Fmt.bytes(v, root.loc);
                                const r = [[I18n.tr("State"), Words.stateName(d.state) + (d.wchan && d.wchan !== "0" && d.state === "D" ? " (" + d.wchan + ")" : "")], [I18n.tr("Program"), d.exe || "–"], [I18n.tr("Folder"), d.cwd || "–"], [I18n.tr("Priority"), String(d.nice)], [I18n.tr("Memory"), I18n.tr("%1 own · %2 files · %3 shared", b(d.anon), b(d.file), b(d.shmem))], [I18n.tr("Peak"), b(d.vmPeak)], [I18n.tr("Open files"), d.fds ? String(d.fds) : "–"], [I18n.tr("Libraries"), String(d.libraries)], [I18n.tr("Read / written"), b(d.ioRead) + " / " + b(d.ioWrite)], [I18n.tr("Context switches"), (d.ctxtVoluntary ?? 0) + " / " + (d.ctxtInvoluntary ?? 0)]];
                                if (d.oomScore !== null)
                                    r.push([I18n.tr("OOM score"), String(d.oomScore)]);
                                if (d.ports.length > 0)
                                    r.push([I18n.tr("Listens on"), d.ports.map(p => p.proto.toUpperCase() + " " + p.port + (p.everywhere ? " ⚠" : "")).join(", ")]);
                                if (d.deleted.length > 0)
                                    r.push([I18n.tr("Outdated files"), d.deleted.slice(0, 5).join("\n")]);
                                return r;
                            }

                            RowLayout {
                                required property var modelData

                                Layout.fillWidth: true
                                spacing: Theme.spacing.md

                                StyledText {
                                    Layout.preferredWidth: 130
                                    Layout.alignment: Qt.AlignTop
                                    text: parent.modelData[0]
                                    color: Theme.colors.textMuted
                                    font.pixelSize: Theme.font.small + 1
                                }

                                StyledText {
                                    Layout.fillWidth: true
                                    text: parent.modelData[1]
                                    font.pixelSize: Theme.font.small + 1
                                    wrapMode: Text.WrapAnywhere
                                    maximumLineCount: 6
                                }
                            }
                        }

                        SectionLabel {
                            visible: (root.d?.threads ?? []).length > 1
                            text: I18n.tr("Busiest threads")
                        }

                        Flow {
                            Layout.fillWidth: true
                            spacing: 4
                            visible: (root.d?.threads ?? []).length > 1

                            Repeater {
                                model: (root.d?.threads ?? []).slice(0, 12)

                                Chip {
                                    required property var modelData

                                    text: modelData.name + " · " + modelData.tid
                                    tint: modelData.state === "R" ? Theme.pulse.cpu : Theme.colors.textMuted
                                }
                            }
                        }
                    }
                }
            }

            // Actions
            Flow {
                Layout.fillWidth: true
                spacing: Theme.spacing.xs
                visible: root.app !== null && root.app.kind !== "kernel"

                PillButton {
                    visible: (root.app?.windows ?? []).length > 0
                    icon: "focus"
                    text: I18n.tr("Switch to")
                    onClicked: Pulse.focus(root.app.windows[0].address)
                }

                PillButton {
                    icon: "restart_alt"
                    text: I18n.tr("Restart")
                    visible: root.app?.kind !== "system" || (root.app?.unit ?? "").endsWith(".service")
                    onClicked: PulseUi.restartApp(root.app.key)
                }

                PillButton {
                    visible: root.app?.kind !== "system"
                    icon: (root.app?.flags ?? []).indexOf("paused") >= 0 ? "play_arrow" : "pause"
                    text: (root.app?.flags ?? []).indexOf("paused") >= 0 ? I18n.tr("Resume") : I18n.tr("Pause")
                    onClicked: Pulse.setPaused(root.app.key, root.app.flags.indexOf("paused") < 0)
                }

                PillButton {
                    visible: root.app?.kind !== "system"
                    icon: "energy_savings_leaf"
                    text: (root.app?.flags ?? []).indexOf("efficiency") >= 0 ? I18n.tr("Normal mode") : I18n.tr("Efficiency")
                    onClicked: Pulse.setEfficiency(root.app.key, root.app.flags.indexOf("efficiency") < 0)
                }

                PillButton {
                    style: "filled"
                    icon: "close"
                    text: root.app?.kind === "system" ? I18n.tr("Stop") : I18n.tr("End task")
                    onClicked: PulseUi.endApp(root.app.key, false)
                }

                PillButton {
                    visible: root.app?.kind !== "system" || root.app?.uid !== 0
                    style: "danger"
                    icon: "stop_circle"
                    text: I18n.tr("Force quit")
                    onClicked: PulseUi.endApp(root.app.key, true)
                }
            }
        }
    }
}
