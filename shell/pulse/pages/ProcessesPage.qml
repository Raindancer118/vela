import QtQuick
import QtQuick.Layouts
import qs
import qs.components
import qs.pulse
import "../Fmt.js" as Fmt
import "../Model.js" as Model

// Windows 11 style process table: apps grouped (with windows, in the
// background, in terminals, services, system), each unfolding into its
// processes; or all processes as a list or tree. Cells are tinted by load,
// rows glide when the order changes.
Item {
    id: page

    property string mode: "grouped"
    property string sortBy: "cpu"
    property bool desc: true
    property var expanded: ({})
    property var collapsed: ({
            kernel: true
        })
    property string selected: ""
    // Row key → { type, depth, kind, count }.
    property var info: ({})
    readonly property var loc: Qt.locale()
    readonly property var f: Pulse.frame
    readonly property var kinds: ["window", "background", "task", "service", "system", "kernel"]
    readonly property var cols: [
        {
            k: "cpu",
            t: I18n.tr("CPU")
        },
        {
            k: "mem",
            t: I18n.tr("Memory")
        },
        {
            k: "disk",
            t: I18n.tr("Disk")
        },
        {
            k: "gpu",
            t: I18n.tr("GPU")
        }
    ]

    Component.onCompleted: {
        Pulse.procWatchers++;
        rebuild();
    }
    Component.onDestruction: Pulse.procWatchers--

    onModeChanged: rebuild()
    onSortByChanged: rebuild()
    onDescChanged: rebuild()
    onExpandedChanged: rebuild()
    onCollapsedChanged: rebuild()

    Connections {
        target: VelaConfig

        function onPulseChanged(): void {
            page.rebuild();
        }
    }

    Connections {
        target: Pulse

        function onFrameArrived(): void {
            page.rebuild();
        }
    }

    Connections {
        target: PulseUi

        function onQueryChanged(): void {
            page.rebuild();
        }
    }

    // Value of a column for an app or a process.
    function metric(o: var, col: string): real {
        if (!o)
            return 0;
        const ncpu = page.f?.cpu.logical ?? 1;
        switch (col) {
        case "cpu":
            return o.pid !== undefined ? o.cpu / ncpu : o.cpu;
        case "mem":
            return o.mem;
        case "disk":
            return o.readBps + o.writeBps;
        case "gpu":
            return o.gpu;
        }
        return 0;
    }

    function heat(col: string, v: real): real {
        const total = page.f?.memory.total ?? 1;
        switch (col) {
        case "cpu":
            return Fmt.heat(v, 40);
        case "mem":
            return Fmt.heat(v, total * 0.2);
        case "disk":
            return Fmt.heat(v, 50 * 1024 * 1024);
        case "gpu":
            return Fmt.heat(v, 60);
        }
        return 0;
    }

    function fmt(col: string, v: real): string {
        switch (col) {
        case "cpu":
            return Fmt.percent(v, loc);
        case "mem":
            return Fmt.bytes(v, loc);
        case "disk":
            return v < 1 ? "0 MB/s" : Fmt.rate(v, loc);
        case "gpu":
            return Fmt.percent(v, loc);
        }
        return "";
    }

    function cmp(a: var, b: var): int {
        if (sortBy === "name") {
            const n = (a.name ?? a.comm).localeCompare(b.name ?? b.comm);
            return desc ? -n : n;
        }
        const d = metric(b, sortBy) - metric(a, sortBy);
        return (desc ? d : -d) || ((a.name ?? a.comm) < (b.name ?? b.comm) ? -1 : 1);
    }

    function procMatches(p: var, q: string): bool {
        return Model.matches(q, [p.comm, String(p.pid), p.user, (p.cmdline ?? []).join(" ")]);
    }

    function rebuild(): void {
        const q = PulseUi.query;
        const keys = [];
        const inf = {};
        if (mode === "grouped") {
            for (const kind of kinds) {
                if (kind === "kernel" && !VelaConfig.pulse.showKernel)
                    continue;
                const list = Pulse.apps.filter(a => a.kind === kind && (Model.matches(q, [a.name, a.unit, (a.command ?? []).join(" ")]) || a.pids.some(pid => Pulse.procMap[pid] && procMatches(Pulse.procMap[pid], q))));
                if (list.length === 0)
                    continue;
                list.sort(cmp);
                const h = "h:" + kind;
                keys.push(h);
                inf[h] = {
                    type: "h",
                    kind: kind,
                    count: list.length
                };
                if (collapsed[kind] && q === "")
                    continue;
                for (const a of list) {
                    const k = "a:" + a.key;
                    keys.push(k);
                    inf[k] = {
                        type: "a",
                        key: a.key,
                        depth: 0
                    };
                    if (expanded[a.key]) {
                        const ps = a.pids.map(pid => Pulse.procMap[pid]).filter(p => p);
                        ps.sort(cmp);
                        for (const p of ps) {
                            const pk = "p:" + p.pid;
                            keys.push(pk);
                            inf[pk] = {
                                type: "p",
                                pid: p.pid,
                                depth: 1
                            };
                        }
                    }
                }
            }
        } else if (mode === "list") {
            const ps = Pulse.procs.filter(p => (VelaConfig.pulse.showKernel || !p.kernel) && procMatches(p, q));
            ps.sort(cmp);
            for (const p of ps) {
                const pk = "p:" + p.pid;
                keys.push(pk);
                inf[pk] = {
                    type: "p",
                    pid: p.pid,
                    depth: 0
                };
            }
        } else {
            // Tree: children below their parent, siblings sorted.
            const kids = {};
            const pids = {};
            const all = Pulse.procs.filter(p => VelaConfig.pulse.showKernel || !p.kernel);
            for (const p of all)
                pids[p.pid] = true;
            for (const p of all) {
                const parent = pids[p.ppid] ? p.ppid : 0;
                (kids[parent] = kids[parent] || []).push(p);
            }
            const show = {};
            if (q !== "") {
                // Keep matches and the chain above them.
                for (const p of Pulse.procs)
                    if (procMatches(p, q)) {
                        let cur = p;
                        while (cur && !show[cur.pid]) {
                            show[cur.pid] = true;
                            cur = Pulse.procMap[cur.ppid];
                        }
                    }
            }
            const walk = (parent, depth) => {
                const list = (kids[parent] || []).slice().sort(cmp);
                for (const p of list) {
                    if (q !== "" && !show[p.pid])
                        continue;
                    const pk = "p:" + p.pid;
                    keys.push(pk);
                    inf[pk] = {
                        type: "p",
                        pid: p.pid,
                        depth: Math.min(depth, 12),
                        kids: (kids[p.pid] || []).length
                    };
                    if (!collapsed["pid:" + p.pid])
                        walk(p.pid, depth + 1);
                }
            };
            walk(0, 0);
        }
        info = inf;
        Model.sync(rows, keys);
    }

    function toggle(obj: var, key: string): var {
        const o = Object.assign({}, obj);
        if (o[key])
            delete o[key];
        else
            o[key] = true;
        return o;
    }

    function sortOn(col: string): void {
        if (sortBy === col)
            desc = !desc;
        else {
            sortBy = col;
            desc = col !== "name";
        }
    }

    readonly property var selectedApp: selected.startsWith("a:") ? Pulse.app(selected.slice(2)) : null
    readonly property var selectedProc: {
        Pulse.revision;
        return selected.startsWith("p:") ? Pulse.procMap[+selected.slice(2)] ?? null : null;
    }

    ListModel {
        id: rows
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.leftMargin: Theme.spacing.xl
        anchors.rightMargin: Theme.spacing.xl
        anchors.topMargin: Theme.spacing.md
        spacing: Theme.spacing.sm

        // Toolbar: view mode and actions on the selection.
        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.spacing.sm

            Rectangle {
                implicitHeight: 32
                implicitWidth: segRow.implicitWidth + 6
                radius: 16
                color: Theme.withAlpha(Theme.colors.text, 0.05)

                readonly property var modes: [
                    {
                        k: "grouped",
                        t: I18n.tr("Apps"),
                        i: "apps"
                    },
                    {
                        k: "list",
                        t: I18n.tr("All processes"),
                        i: "list"
                    },
                    {
                        k: "tree",
                        t: I18n.tr("Tree"),
                        i: "sort"
                    }
                ]

                Rectangle {
                    readonly property Item target: segRepeater.itemAt(parent.modes.findIndex(m => m.k === page.mode))
                    x: 3 + (target?.x ?? 0)
                    y: 3
                    width: target?.width ?? 0
                    height: parent.height - 6
                    radius: height / 2
                    color: Theme.colors.selected
                    border.width: 1
                    border.color: Theme.colors.selectedRing
                    Behavior on x {
                        SpringAnim {}
                    }
                    Behavior on width {
                        SpringAnim {}
                    }
                }

                Row {
                    id: segRow

                    x: 3
                    y: 3
                    height: parent.height - 6

                    Repeater {
                        id: segRepeater

                        model: parent.parent.modes

                        Item {
                            required property var modelData

                            width: segInner.implicitWidth + 2 * Theme.spacing.md
                            height: parent.height

                            Clickable {
                                radius: height / 2
                                onClicked: page.mode = parent.modelData.k
                            }

                            RowLayout {
                                id: segInner

                                anchors.centerIn: parent
                                spacing: Theme.spacing.xs

                                MaterialIcon {
                                    icon: parent.parent.modelData.i
                                    size: Theme.icon.small
                                    color: page.mode === parent.parent.modelData.k ? Theme.colors.primary : Theme.colors.textMuted
                                }

                                StyledText {
                                    text: parent.parent.modelData.t
                                    font.pixelSize: Theme.font.small + 1
                                }
                            }
                        }
                    }
                }
            }

            Item {
                Layout.fillWidth: true
            }

            PillButton {
                visible: page.selectedApp !== null && page.selectedApp.kind !== "kernel" && page.selectedApp.kind !== "system"
                icon: "energy_savings_leaf"
                text: (page.selectedApp?.flags ?? []).indexOf("efficiency") >= 0 ? I18n.tr("Leave efficiency mode") : I18n.tr("Efficiency mode")
                onClicked: Pulse.setEfficiency(page.selectedApp.key, page.selectedApp.flags.indexOf("efficiency") < 0)
            }

            PillButton {
                visible: page.selectedApp !== null && page.selectedApp.kind !== "kernel"
                icon: "restart_alt"
                text: I18n.tr("Restart")
                onClicked: PulseUi.restartApp(page.selectedApp.key)
            }

            PillButton {
                visible: page.selectedApp !== null || page.selectedProc !== null
                icon: "info_outline"
                text: I18n.tr("Details")
                onClicked: {
                    if (page.selectedApp)
                        PulseUi.showApp(page.selectedApp.key);
                    else {
                        PulseUi.showApp(page.selectedProc.app);
                        PulseUi.detailPid = page.selectedProc.pid;
                    }
                }
            }

            PillButton {
                style: "filled"
                icon: "close"
                enabled: page.selectedApp !== null && page.selectedApp.kind !== "kernel" || page.selectedProc !== null
                text: page.selectedProc ? I18n.tr("End process") : I18n.tr("End task")
                onClicked: {
                    if (page.selectedApp)
                        PulseUi.endApp(page.selectedApp.key, false);
                    else if (page.selectedProc)
                        Pulse.signalPids([page.selectedProc.pid], "TERM");
                }
            }
        }

        // Column header with totals (like Windows 11).
        Item {
            Layout.fillWidth: true
            implicitHeight: 46

            RowLayout {
                anchors.fill: parent
                spacing: 0

                Item {
                    Layout.fillWidth: true
                    Layout.fillHeight: true

                    Clickable {
                        radius: Theme.radius.small
                        onClicked: page.sortOn("name")
                    }

                    RowLayout {
                        anchors.fill: parent
                        anchors.leftMargin: Theme.spacing.md

                        StyledText {
                            text: I18n.tr("Name")
                            color: Theme.colors.textMuted
                            font.weight: Theme.font.weightMedium
                        }

                        MaterialIcon {
                            visible: page.sortBy === "name"
                            icon: "expand_more"
                            size: Theme.icon.small
                            color: Theme.colors.textMuted
                            rotation: page.desc ? 0 : 180
                            Behavior on rotation {
                                SpringAnim {}
                            }
                        }

                        Item {
                            Layout.fillWidth: true
                        }
                    }
                }

                StyledText {
                    visible: page.mode !== "grouped"
                    Layout.preferredWidth: 72
                    text: I18n.tr("PID")
                    color: Theme.colors.textMuted
                    font.weight: Theme.font.weightMedium
                }

                StyledText {
                    Layout.preferredWidth: 230
                    text: I18n.tr("Status")
                    color: Theme.colors.textMuted
                    font.weight: Theme.font.weightMedium
                }

                Repeater {
                    model: page.cols

                    Item {
                        id: colHead

                        required property var modelData
                        readonly property real total: {
                            const f = page.f;
                            if (!f)
                                return 0;
                            switch (modelData.k) {
                            case "cpu":
                                return f.cpu.usage;
                            case "mem":
                                return f.memory.total > 0 ? f.memory.used / f.memory.total * 100 : 0;
                            case "disk":
                                return f.disks.reduce((m, d) => Math.max(m, d.busy), 0);
                            case "gpu":
                                return f.gpus.reduce((m, g) => Math.max(m, g.busy ?? 0), 0);
                            }
                            return 0;
                        }

                        Layout.preferredWidth: Theme.pulse.columnWidth
                        Layout.fillHeight: true

                        Clickable {
                            radius: Theme.radius.small
                            onClicked: page.sortOn(colHead.modelData.k)
                        }

                        Rectangle {
                            anchors.bottom: parent.bottom
                            anchors.horizontalCenter: parent.horizontalCenter
                            width: page.sortBy === colHead.modelData.k ? parent.width - 16 : 0
                            height: 2
                            radius: 1
                            color: Theme.colors.primary
                            Behavior on width {
                                SpringAnim {}
                            }
                        }

                        ColumnLayout {
                            anchors.right: parent.right
                            anchors.rightMargin: Theme.spacing.sm
                            anchors.verticalCenter: parent.verticalCenter
                            spacing: 0

                            Counter {
                                Layout.alignment: Qt.AlignRight
                                value: colHead.total
                                format: v => Fmt.percent(v, page.loc, 0)
                                font.pixelSize: Theme.font.title
                                font.weight: Theme.font.weightSemiBold
                                color: colHead.total >= 85 ? Theme.pulse.crit : Theme.colors.text
                            }

                            StyledText {
                                Layout.alignment: Qt.AlignRight
                                text: colHead.modelData.t
                                color: Theme.colors.textMuted
                                font.pixelSize: Theme.font.small
                            }
                        }
                    }
                }

                Item {
                    Layout.preferredWidth: 76
                }
            }

            Rectangle {
                anchors.bottom: parent.bottom
                width: parent.width
                height: 1
                color: Theme.colors.divider
            }
        }

        ListView {
            id: list

            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            model: rows
            boundsBehavior: Flickable.StopAtBounds
            reuseItems: false
            cacheBuffer: 400

            add: Transition {
                Anim {
                    property: "opacity"
                    from: 0
                    to: 1
                }
                Anim {
                    property: "x"
                    from: -12
                    to: 0
                    easing.bezierCurve: Theme.anim.emphasizedDecel
                }
            }
            remove: Transition {
                ParallelAnimation {
                    Anim {
                        property: "opacity"
                        to: 0
                        duration: Theme.anim.normal
                    }
                    Anim {
                        property: "x"
                        to: 40
                        duration: Theme.anim.normal
                    }
                }
            }
            displaced: Transition {
                SpringAnim {
                    property: "y"
                    duration: Theme.anim.normal
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
            move: Transition {
                SpringAnim {
                    property: "y"
                    duration: Theme.anim.normal
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
                    duration: Theme.anim.normal
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

            Rectangle {
                anchors.right: parent.right
                visible: list.contentHeight > list.height
                y: list.visibleArea.yPosition * list.height
                width: Theme.size.scrollbarWidth
                height: list.visibleArea.heightRatio * list.height
                radius: width / 2
                color: Theme.colors.textDisabled
                opacity: list.moving ? 1 : Theme.opacity.scrollbarIdle
            }

            delegate: Item {
                id: row

                required property string key
                required property int index
                readonly property var i: page.info[key] ?? {
                    type: "h"
                }
                readonly property bool header: i.type === "h"
                readonly property var app: i.type === "a" ? Pulse.app(i.key) : null
                readonly property var proc: {
                    Pulse.revision;
                    return i.type === "p" ? Pulse.procMap[i.pid] ?? null : null;
                }
                readonly property var obj: app ?? proc
                readonly property string busy: app ? (Pulse.busy[app.key] ?? "") : ""
                readonly property bool isSelected: page.selected === key
                readonly property var flags: obj?.flags ?? []

                width: ListView.view.width
                height: header ? Theme.pulse.sectionHeight + (index > 0 ? Theme.spacing.sm : 0) : Theme.pulse.rowHeight

                // Section header
                Item {
                    visible: row.header
                    anchors.fill: parent
                    anchors.topMargin: row.index > 0 ? Theme.spacing.sm : 0

                    Clickable {
                        radius: Theme.radius.small
                        onClicked: page.collapsed = page.toggle(page.collapsed, row.i.kind)
                    }

                    RowLayout {
                        anchors.fill: parent
                        anchors.leftMargin: Theme.spacing.xs
                        spacing: Theme.spacing.sm

                        MaterialIcon {
                            icon: "expand_more"
                            size: Theme.icon.small
                            color: Theme.colors.textMuted
                            rotation: page.collapsed[row.i.kind] && PulseUi.query === "" ? -90 : 0
                            Behavior on rotation {
                                SpringAnim {}
                            }
                        }

                        StyledText {
                            text: Words.kind(row.i.kind ?? "")
                            font.weight: Theme.font.weightSemiBold
                        }

                        StyledText {
                            text: "(" + (row.i.count ?? 0) + ")"
                            color: Theme.colors.textMuted
                        }

                        Item {
                            Layout.fillWidth: true
                        }
                    }
                }

                // App or process row
                Rectangle {
                    visible: !row.header
                    anchors.fill: parent
                    radius: Theme.radius.small
                    color: row.isSelected ? Theme.colors.selected : rowMouse.containsMouse ? Theme.colors.hover : "transparent"
                    border.width: row.isSelected ? 1 : 0
                    border.color: Theme.colors.selectedRing
                    opacity: row.busy !== "" ? 0.6 : 1
                    Behavior on color {
                        ColorAnim {
                            duration: Theme.anim.fast
                        }
                    }
                    Behavior on opacity {
                        Anim {}
                    }

                    // Busy shimmer while an action runs on the app.
                    Rectangle {
                        visible: row.busy !== ""
                        width: parent.width * 0.3
                        height: parent.height
                        radius: parent.radius
                        gradient: Gradient {
                            orientation: Gradient.Horizontal
                            GradientStop {
                                position: 0
                                color: "transparent"
                            }
                            GradientStop {
                                position: 0.5
                                color: Theme.withAlpha(Theme.colors.primary, 0.18)
                            }
                            GradientStop {
                                position: 1
                                color: "transparent"
                            }
                        }
                        NumberAnimation on x {
                            running: row.busy !== ""
                            loops: Animation.Infinite
                            from: -row.width * 0.3
                            to: row.width
                            duration: 1100
                        }
                    }

                    MouseArea {
                        id: rowMouse

                        anchors.fill: parent
                        hoverEnabled: true
                        acceptedButtons: Qt.LeftButton | Qt.RightButton
                        onClicked: mouse => {
                            page.selected = row.key;
                            if (mouse.button === Qt.RightButton) {
                                const p = mapToItem(null, mouse.x, mouse.y);
                                PulseUi.openMenu(p.x, p.y, row.app ? PulseUi.appMenu(row.app) : PulseUi.procMenu(row.proc));
                            }
                        }
                        onDoubleClicked: {
                            if (row.app)
                                PulseUi.showApp(row.app.key);
                            else if (row.proc) {
                                PulseUi.showApp(row.proc.app);
                                PulseUi.detailPid = row.proc.pid;
                            }
                        }
                    }

                    RowLayout {
                        anchors.fill: parent
                        spacing: 0

                        RowLayout {
                            Layout.fillWidth: true
                            Layout.leftMargin: Theme.spacing.sm + (row.i.depth ?? 0) * (page.mode === "tree" ? 16 : 28)
                            spacing: Theme.spacing.sm

                            // Unfold an app into its processes (or a tree node).
                            Item {
                                implicitWidth: 18
                                implicitHeight: 18
                                visible: row.app !== null || page.mode === "tree"

                                MaterialIcon {
                                    anchors.centerIn: parent
                                    visible: row.app ? row.app.pids.length > 0 : (row.i.kids ?? 0) > 0
                                    icon: "expand_more"
                                    size: Theme.icon.small
                                    color: Theme.colors.textMuted
                                    rotation: (row.app ? page.expanded[row.app.key] : !page.collapsed["pid:" + (row.proc?.pid ?? 0)]) ? 0 : -90
                                    Behavior on rotation {
                                        SpringAnim {}
                                    }
                                }

                                MouseArea {
                                    anchors.fill: parent
                                    anchors.margins: -6
                                    onClicked: {
                                        if (row.app)
                                            page.expanded = page.toggle(page.expanded, row.app.key);
                                        else if (row.proc)
                                            page.collapsed = page.toggle(page.collapsed, "pid:" + row.proc.pid);
                                    }
                                }
                            }

                            AppIcon {
                                size: row.app ? 22 : 18
                                icon: row.app?.icon ?? (row.proc ? (Pulse.app(row.proc.app)?.icon ?? "") : "")
                                name: row.app?.name ?? row.proc?.comm ?? ""
                                fallback: (row.app?.kind ?? Pulse.app(row.proc?.app ?? "")?.kind) === "task" ? "utilities-terminal" : "application-x-executable"
                                opacity: row.proc && page.mode === "grouped" ? 0.7 : 1
                            }

                            StyledText {
                                Layout.fillWidth: true
                                text: {
                                    if (row.app)
                                        return row.app.name + (row.app.pids.length > 1 ? "  (" + row.app.pids.length + ")" : "");
                                    if (!row.proc)
                                        return "";
                                    const args = (row.proc.cmdline ?? []).slice(1).join(" ");
                                    return row.proc.comm + (page.mode === "grouped" && args ? "  " + args : "");
                                }
                                color: row.proc && page.mode === "grouped" ? Theme.colors.textMuted : Theme.colors.text
                                font.pixelSize: row.proc && page.mode === "grouped" ? Theme.font.small + 1 : Theme.font.body
                            }
                        }

                        StyledText {
                            visible: page.mode !== "grouped"
                            Layout.preferredWidth: 72
                            text: row.proc?.pid ?? ""
                            color: Theme.colors.textMuted
                            font.features: { "tnum": 1 }
                        }

                        // Status: a running action, flags, or the process state.
                        Item {
                            Layout.preferredWidth: 230
                            Layout.fillHeight: true

                            Row {
                                anchors.verticalCenter: parent.verticalCenter
                                width: parent.width - Theme.spacing.sm
                                clip: true
                                spacing: 4

                                Chip {
                                    visible: row.busy !== ""
                                    text: ({
                                            end: I18n.tr("Ending…"),
                                            force: I18n.tr("Ending…"),
                                            restart: I18n.tr("Restarting…"),
                                            efficiency: I18n.tr("Applying…"),
                                            pause: I18n.tr("Pausing…"),
                                            resume: I18n.tr("Resuming…")
                                        })[row.busy] ?? I18n.tr("Working…")
                                    tint: Theme.colors.primary
                                }

                                Repeater {
                                    model: row.busy === "" ? row.flags.slice(0, 2) : []

                                    Chip {
                                        required property string modelData

                                        text: Words.flag(modelData)
                                        icon: Words.flagIcon(modelData)
                                        tint: Words.flagColor(modelData)
                                    }
                                }

                                StyledText {
                                    visible: row.proc !== null && row.flags.length === 0 && row.busy === ""
                                    text: row.proc ? Words.stateName(row.proc.state) : ""
                                    color: Theme.colors.textMuted
                                    font.pixelSize: Theme.font.small
                                }
                            }
                        }

                        Repeater {
                            model: page.cols

                            Rectangle {
                                id: cell

                                required property var modelData
                                readonly property real v: page.metric(row.obj, modelData.k)
                                readonly property real h: page.heat(modelData.k, v)

                                Layout.preferredWidth: Theme.pulse.columnWidth
                                Layout.fillHeight: true
                                Layout.topMargin: 2
                                Layout.bottomMargin: 2
                                radius: 4
                                color: Theme.withAlpha(cell.h > 0.75 ? Theme.pulse.heatHot : Theme.pulse.heat, VelaConfig.pulse.heatMap ? Theme.pulse.heatMax * cell.h : 0)
                                Behavior on color {
                                    ColorAnim {
                                        duration: Theme.anim.slow
                                    }
                                }

                                StyledText {
                                    anchors.right: parent.right
                                    anchors.rightMargin: Theme.spacing.sm
                                    anchors.verticalCenter: parent.verticalCenter
                                    text: row.obj ? page.fmt(cell.modelData.k, cell.v) : ""
                                    font.features: { "tnum": 1 }
                                    font.pixelSize: row.proc && page.mode === "grouped" ? Theme.font.small + 1 : Theme.font.body
                                    color: cell.v <= 0 ? Theme.colors.textMuted : Theme.colors.text
                                }
                            }
                        }

                        // Hover actions.
                        Row {
                            Layout.preferredWidth: 76
                            layoutDirection: Qt.RightToLeft
                            spacing: 2
                            opacity: rowMouse.containsMouse || row.isSelected ? 1 : 0
                            Behavior on opacity {
                                Anim {
                                    duration: Theme.anim.fast
                                }
                            }

                            IconButton {
                                implicitWidth: 30
                                implicitHeight: 30
                                icon: "more"
                                iconSize: Theme.icon.small

                                Tip {
                                    text: I18n.tr("More")
                                    shown: parent.hovered
                                }
                                onClicked: {
                                    const p = mapToItem(null, 0, height);
                                    page.selected = row.key;
                                    PulseUi.openMenu(p.x - 200, p.y, row.app ? PulseUi.appMenu(row.app) : PulseUi.procMenu(row.proc));
                                }
                            }

                            IconButton {
                                visible: (row.app && row.app.kind !== "kernel") || row.proc !== null
                                implicitWidth: 30
                                implicitHeight: 30
                                icon: "close"
                                iconSize: Theme.icon.small
                                iconColor: hovered ? Theme.pulse.crit : Theme.colors.textMuted

                                Tip {
                                    text: I18n.tr("End")
                                    shown: parent.hovered
                                }
                                onClicked: {
                                    if (row.app)
                                        PulseUi.endApp(row.app.key, false);
                                    else
                                        Pulse.signalPids([row.proc.pid], "TERM");
                                }
                            }
                        }
                    }
                }
            }

            StyledText {
                anchors.centerIn: parent
                visible: rows.count === 0 && Pulse.frame !== null
                text: PulseUi.query !== "" ? I18n.tr("Nothing matches “%1”", PulseUi.query) : I18n.tr("Loading processes…")
                color: Theme.colors.textMuted
            }
        }
    }
}
