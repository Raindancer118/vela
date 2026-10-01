import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Hyprland
import Quickshell.Io
import Quickshell.Wayland
import qs
import qs.components
import "logic.js" as Logic

// Screen-share picker for xdg-desktop-portal-hyprland, run by
// `vela share-picker` as its own Quickshell instance (share-picker.qml).
// The answer goes as JSON to $VELA_SHARE_OUT; quitting without one cancels.
Scope {
    id: root

    readonly property string outPath: Quickshell.env("VELA_SHARE_OUT") ?? ""
    property bool token: Quickshell.env("VELA_SHARE_TOKEN") === "1"
    // Test hook for screenshots: opens on that tab without taking the
    // keyboard and quits after a few seconds, with VELA_SHARE_PREVIEW_SHARE=1
    // sharing the preselection (end-to-end test through xdph).
    readonly property string previewTab: Quickshell.env("VELA_SHARE_PREVIEW") ?? ""

    // "screen", "window" or "region".
    property string tab: "screen"
    readonly property var tabs: ["screen", "window", "region"]
    property string screenName: ""
    property int windowIndex: -1
    property string query: ""
    property bool finishing: false
    // Shown once vela's colours have arrived (no flash of the defaults).
    property bool ready: false

    readonly property var xdphWindows: {
        try {
            return JSON.parse(Quickshell.env("VELA_SHARE_WINDOWS") || "[]");
        } catch (e) {
            return [];
        }
    }
    readonly property var windows: Logic.sortWindows(xdphWindows.map(w => {
        const t = w.address !== "" ? Hyprland.toplevels.values.find(t => t.address === w.address) ?? null : null;
        const ws = t?.workspace ?? null;
        return {
            handle: w.handle,
            title: t?.title || w.title || w["class"],
            className: w["class"],
            toplevel: t?.wayland ?? null,
            workspaceId: ws?.id ?? 0,
            workspaceName: ws?.name ?? ""
        };
    }))
    readonly property var shownWindows: Logic.filterWindows(windows, query)
    readonly property var selectedWindow: shownWindows[windowIndex] ?? null

    readonly property var focusedMonitor: Hyprland.monitors.values.find(m => m.focused) ?? Hyprland.focusedMonitor
    readonly property ShellScreen hostScreen: Quickshell.screens.find(s => s.name === focusedMonitor?.name) ?? Quickshell.screens[0] ?? null

    readonly property bool canShare: tab === "region" || (tab === "screen" && screenName !== "") || (tab === "window" && selectedWindow !== null)

    function share(): void {
        if (!canShare)
            return;
        if (tab === "screen")
            finish(Logic.answer("screen", screenName, token));
        else if (tab === "window")
            finish(Logic.answer("window", selectedWindow.handle, token));
        else
            finish(Logic.answer("region", "", token));
    }

    function finish(answer: string): void {
        if (finishing)
            return;
        finishing = true;
        if (answer !== "" && outPath !== "")
            out.setText(answer + "\n");
        // Gone before slurp starts for a region; the fade is the rest.
        quitTimer.interval = answer !== "" && tab === "region" ? 0 : Theme.anim.fast;
        quitTimer.start();
    }

    function setTab(name: string): void {
        tab = name;
        if (name === "window" && windowIndex < 0 && shownWindows.length > 0)
            windowIndex = 0;
    }

    function cycleTab(step: int): void {
        setTab(tabs[(tabs.indexOf(tab) + step + tabs.length) % tabs.length]);
    }

    function workspaceLabel(win: var): string {
        if (win.workspaceName === "special:minimized")
            return I18n.tr("Minimized");
        if (win.workspaceName.startsWith("special"))
            return win.workspaceName.split(":")[1] || I18n.tr("Special workspace");
        return win.workspaceId > 0 ? I18n.tr("Workspace %1", win.workspaceName || win.workspaceId) : "";
    }

    // Keys shared by the window and the search field (which has the focus on
    // the window tab, so typing filters right away).
    function handleKey(event: var): void {
        const k = event.key;
        const arrows = {
            [Qt.Key_Left]: "left",
            [Qt.Key_Right]: "right",
            [Qt.Key_Up]: "up",
            [Qt.Key_Down]: "down"
        };
        if (k === Qt.Key_Escape) {
            if (tab === "window" && query !== "")
                query = "";
            else
                finish("");
        } else if (k === Qt.Key_Return || k === Qt.Key_Enter) {
            share();
        } else if (k === Qt.Key_Tab || k === Qt.Key_Backtab) {
            cycleTab(k === Qt.Key_Backtab || (event.modifiers & Qt.ShiftModifier) ? -1 : 1);
        } else if ((event.modifiers & Qt.ControlModifier) && k >= Qt.Key_1 && k <= Qt.Key_3) {
            setTab(tabs[k - Qt.Key_1]);
        } else if ((event.modifiers & Qt.ControlModifier) && k === Qt.Key_R) {
            token = !token;
        } else if (arrows[k] !== undefined) {
            if (tab === "screen")
                screenView.move(arrows[k]);
            else if (tab === "window")
                windowIndex = Logic.gridMove(windowIndex, shownWindows.length, windowGrid.columns, arrows[k]);
        } else {
            return;
        }
        event.accepted = true;
    }

    onQueryChanged: windowIndex = shownWindows.length > 0 ? 0 : -1
    onFocusedMonitorChanged: {
        if (screenName === "" && focusedMonitor)
            screenName = focusedMonitor.name;
    }
    onHostScreenChanged: {
        if (screenName === "" && hostScreen)
            screenName = hostScreen.name;
    }

    Component.onCompleted: {
        Hyprland.refreshMonitors();
        Hyprland.refreshToplevels();
        screenName = focusedMonitor?.name ?? hostScreen?.name ?? "";
        if (previewTab !== "") {
            setTab(previewTab);
            previewQuit.start();
        }
    }

    Timer {
        id: previewQuit

        interval: Quickshell.env("VELA_SHARE_PREVIEW_SHARE") === "1" ? 1500 : 5000
        onTriggered: Quickshell.env("VELA_SHARE_PREVIEW_SHARE") === "1" ? root.share() : root.finish("")
    }

    Connections {
        target: VelaConfig

        function onLoadedChanged(): void {
            root.ready = true;
        }
    }

    // Without vela's settings (old vela, broken config) after a moment anyway.
    Timer {
        running: true
        interval: 600
        onTriggered: root.ready = true
    }

    Timer {
        id: quitTimer

        onTriggered: Qt.quit()
    }

    FileView {
        id: out

        path: root.outPath
        blockWrites: true
        atomicWrites: true
    }

    PanelWindow {
        id: win

        property real progress: root.ready && !root.finishing ? 1 : 0

        Behavior on progress {
            Anim {
                duration: root.finishing ? Theme.anim.fast : Theme.anim.slow
                easing.bezierCurve: Theme.anim.emphasizedDecel
            }
        }

        visible: root.hostScreen !== null && (root.ready || progress > 0) && !(root.finishing && progress === 0)
        screen: root.hostScreen
        color: Qt.rgba(0, 0, 0, VelaConfig.appearance.backdropDim * win.progress)
        anchors {
            top: true
            bottom: true
            left: true
            right: true
        }
        exclusionMode: ExclusionMode.Ignore
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "quickshell-share"
        WlrLayershell.keyboardFocus: root.finishing || root.previewTab !== "" ? WlrKeyboardFocus.None : WlrKeyboardFocus.Exclusive

        FocusScope {
            id: scope

            anchors.fill: parent
            focus: true
            Keys.onPressed: event => root.handleKey(event)

            MouseArea {
                anchors.fill: parent
                onClicked: root.finish("")
            }

            Rectangle {
                id: card

                anchors.centerIn: parent
                width: Math.min(parent.width - 2 * Theme.spacing.xl * 2, 980)
                height: Math.min(parent.height - 2 * Theme.spacing.xl * 2, 660)
                radius: Theme.radius.panel
                color: Theme.colors.panel
                border.width: Theme.size.border
                border.color: Theme.colors.outline
                opacity: Math.min(1, win.progress * Theme.anim.fadeLead)
                scale: Theme.anim.popInScale + (1 - Theme.anim.popInScale) * win.progress

                // Clicks on the card don't cancel.
                MouseArea {
                    anchors.fill: parent
                }

                ColumnLayout {
                    anchors.fill: parent
                    anchors.margins: Theme.spacing.xl
                    spacing: Theme.spacing.lg

                    // Header
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: Theme.spacing.md

                        Rectangle {
                            implicitWidth: Theme.size.tileIcon + Theme.spacing.sm
                            implicitHeight: implicitWidth
                            radius: height / 2
                            color: Theme.colors.accentChip

                            MaterialIcon {
                                anchors.centerIn: parent
                                icon: "screen_share"
                                size: Theme.icon.large
                                color: Theme.colors.primary
                            }
                        }

                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 2

                            StyledText {
                                text: I18n.tr("Share your screen")
                                font.pixelSize: Theme.font.large
                                font.weight: Theme.font.weightSemiBold
                            }

                            StyledText {
                                Layout.fillWidth: true
                                text: I18n.tr("An app wants to record your screen. Choose what it may see.")
                                color: Theme.colors.textMuted
                            }
                        }

                        IconButton {
                            icon: "close"
                            onClicked: root.finish("")
                        }
                    }

                    // Tabs and, for windows, the search.
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: Theme.spacing.md

                        Rectangle {
                            id: segments

                            implicitWidth: segRow.implicitWidth + 2 * Theme.spacing.xs
                            implicitHeight: Theme.size.pillButton + 2 * Theme.spacing.xs
                            radius: Theme.radius.small + Theme.spacing.xs
                            color: Theme.colors.chip

                            Rectangle {
                                readonly property Item target: segRow.children[root.tabs.indexOf(root.tab)] ?? null

                                x: segRow.x + (target?.x ?? 0)
                                y: segRow.y
                                width: target?.width ?? 0
                                height: segRow.height
                                radius: Theme.radius.small
                                color: Theme.colors.selected
                                border.width: Theme.size.border
                                border.color: Theme.colors.selectedRing

                                Behavior on x {
                                    SpringAnim {
                                        duration: Theme.anim.normal
                                    }
                                }

                                Behavior on width {
                                    Anim {}
                                }
                            }

                            Row {
                                id: segRow

                                anchors.centerIn: parent

                                Repeater {
                                    model: [
                                        {
                                            name: "screen",
                                            icon: "monitor",
                                            label: I18n.tr("Screen"),
                                            count: Quickshell.screens.length
                                        },
                                        {
                                            name: "window",
                                            icon: "select_window",
                                            label: I18n.tr("Window"),
                                            count: root.windows.length
                                        },
                                        {
                                            name: "region",
                                            icon: "select_region",
                                            label: I18n.tr("Region"),
                                            count: -1
                                        }
                                    ]

                                    Item {
                                        id: seg

                                        required property var modelData
                                        readonly property bool active: root.tab === modelData.name

                                        implicitWidth: segContent.implicitWidth + 2 * Theme.spacing.lg
                                        implicitHeight: Theme.size.pillButton

                                        Clickable {
                                            radius: Theme.radius.small
                                            onClicked: root.setTab(seg.modelData.name)
                                        }

                                        RowLayout {
                                            id: segContent

                                            anchors.centerIn: parent
                                            spacing: Theme.spacing.sm

                                            MaterialIcon {
                                                icon: seg.modelData.icon
                                                size: Theme.icon.normal
                                                color: seg.active ? Theme.colors.primary : Theme.colors.textMuted

                                                Behavior on color {
                                                    ColorAnim {}
                                                }
                                            }

                                            StyledText {
                                                text: seg.modelData.label
                                                font.weight: Theme.font.weightMedium
                                                color: seg.active ? Theme.colors.text : Theme.colors.textMuted
                                            }

                                            Rectangle {
                                                visible: seg.modelData.count >= 0
                                                implicitWidth: Math.max(implicitHeight, countText.implicitWidth + Theme.spacing.sm)
                                                implicitHeight: countText.implicitHeight + 2
                                                radius: height / 2
                                                color: seg.active ? Theme.colors.accentChip : Theme.colors.chip

                                                StyledText {
                                                    id: countText

                                                    anchors.centerIn: parent
                                                    text: seg.modelData.count
                                                    font.pixelSize: Theme.font.small
                                                    font.weight: Theme.font.weightSemiBold
                                                    color: seg.active ? Theme.colors.primary : Theme.colors.textMuted
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        Item {
                            Layout.fillWidth: true
                        }

                        TextField {
                            id: search

                            Layout.preferredWidth: Theme.size.controlWidth + Theme.spacing.xl * 2
                            visible: root.tab === "window" && root.windows.length > 0
                            placeholder: I18n.tr("Search windows")
                            text: root.query
                            onTextChanged: root.query = text
                            input.focus: visible
                            input.Keys.onPressed: event => root.handleKey(event)
                            onVisibleChanged: visible ? input.forceActiveFocus() : scope.forceActiveFocus()
                        }
                    }

                    // Content
                    Item {
                        Layout.fillWidth: true
                        Layout.fillHeight: true

                        ScreenView {
                            id: screenView

                            anchors.fill: parent
                            shown: root.tab === "screen"
                            live: root.ready && !root.finishing
                            selected: root.screenName
                            onPicked: name => root.screenName = name
                            onActivated: name => {
                                root.screenName = name;
                                root.share();
                            }
                        }

                        WindowGrid {
                            id: windowGrid

                            anchors.fill: parent
                            shown: root.tab === "window"
                            live: root.ready && !root.finishing
                            windows: root.shownWindows
                            totalCount: root.windows.length
                            currentIndex: root.windowIndex
                            labelFor: w => root.workspaceLabel(w)
                            onPicked: i => root.windowIndex = i
                            onActivated: i => {
                                root.windowIndex = i;
                                root.share();
                            }
                        }

                        RegionView {
                            anchors.fill: parent
                            shown: root.tab === "region"
                            onActivated: root.share()
                        }
                    }

                    Divider {
                        Layout.fillWidth: true
                    }

                    // Footer
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: Theme.spacing.md

                        Switch {
                            checked: root.token
                            onToggled: root.token = !root.token
                        }

                        ColumnLayout {
                            spacing: 0

                            StyledText {
                                text: I18n.tr("Remember this choice")
                                font.weight: Theme.font.weightMedium
                            }

                            StyledText {
                                text: I18n.tr("The app may share it again without asking")
                                font.pixelSize: Theme.font.small
                                color: Theme.colors.textMuted
                            }
                        }

                        Item {
                            Layout.fillWidth: true
                        }

                        StyledText {
                            text: I18n.tr("Tab switches · Enter shares · Esc cancels")
                            font.pixelSize: Theme.font.small
                            color: Theme.colors.textDisabled
                        }

                        PillButton {
                            text: I18n.tr("Cancel")
                            onClicked: root.finish("")
                        }

                        PillButton {
                            style: "filled"
                            icon: "screen_share"
                            text: root.tab === "region" ? I18n.tr("Select region") : I18n.tr("Share")
                            enabled: root.canShare
                            onClicked: root.share()
                        }
                    }
                }
            }
        }
    }
}
