pragma Singleton

import QtQuick
import Quickshell
import qs

// What the Pulse window shows right now; shared by the pages.
Singleton {
    id: root

    readonly property var pages: ["overview", "processes", "performance", "diagnosis", "services", "activity"]
    readonly property string envPage: Quickshell.env("VELA_PULSE_PAGE") ?? ""
    property string page: pages.indexOf(envPage) >= 0 ? envPage : "overview"
    property bool navigated: false
    onPageChanged: navigated = true

    // Test hooks for screenshots: VELA_PULSE_DETAIL=1 opens the busiest
    // windowed app's details, VELA_PULSE_DIALOG=run|confirm a dialog.
    Connections {
        target: Pulse
        enabled: (Quickshell.env("VELA_PULSE_DETAIL") ?? "") !== "" || (Quickshell.env("VELA_PULSE_DIALOG") ?? "") !== ""

        function onFrameArrived(): void {
            if (Pulse.revision !== 3)
                return;
            if (Quickshell.env("VELA_PULSE_DETAIL")) {
                const a = Pulse.apps.filter(x => x.kind === "window").sort((x, y) => y.mem - x.mem)[0];
                if (a)
                    root.showApp(a.key);
            }
            if (Quickshell.env("VELA_PULSE_DIALOG") === "run")
                root.runOpen = true;
            else if (Quickshell.env("VELA_PULSE_DIALOG") === "confirm")
                root.ask(I18n.tr("Force %1 to quit?", "Firefox"), I18n.tr("It gets no chance to save. Unsaved work in it is lost."), I18n.tr("Force quit"), true, null);
        }
    }

    // The start page from the settings, unless the user already moved on.
    Connections {
        target: VelaConfig

        function onLoadedChanged(): void {
            const p = VelaConfig.pulse.startPage;
            if (root.envPage === "" && !root.navigated && root.pages.indexOf(p) >= 0) {
                root.page = p;
                root.navigated = false;
            }
        }
    }
    // Performance page: cpu, memory, gpu:<card>, disk:<name>, net:<iface>, power, sensors.
    property string device: "cpu"
    property string query: ""
    // App whose details are open (flyout), "" = closed.
    property string detailKey: ""
    property int detailPid: 0
    property bool runOpen: false
    // { title, text, confirm, danger, action: function } or null.
    property var confirm: null
    // Selected row in Apps & processes: "a:<app key>" or "p:<pid>".
    property string selected: ""
    // Context menu: { x, y, items: [{ icon, label, danger, enabled, action }] } or null.
    property var menu: null

    function openMenu(x: real, y: real, items: var): void {
        menu = {
            x: x,
            y: y,
            items: items
        };
    }

    // The actions every app row and the flyout offer.
    function appMenu(a: var): var {
        if (!a)
            return [];
        const sys = a.kind === "system" || a.kind === "kernel";
        const paused = a.flags.indexOf("paused") >= 0;
        const eff = a.flags.indexOf("efficiency") >= 0;
        const items = [];
        if (a.windows.length > 0)
            items.push({
                icon: "focus",
                label: I18n.tr("Switch to"),
                action: () => Pulse.focus(a.windows[0].address)
            });
        items.push({
            icon: "info_outline",
            label: I18n.tr("Details"),
            action: () => showApp(a.key)
        });
        if (a.kind !== "kernel") {
            items.push({
                separator: true
            });
            items.push({
                icon: "close",
                label: a.kind === "system" ? I18n.tr("Stop") : I18n.tr("End task"),
                hint: keyHint("end"),
                action: () => endApp(a.key, false)
            });
            if (!sys || a.uid !== 0)
                items.push({
                    icon: "stop_circle",
                    label: I18n.tr("Force quit"),
                    hint: keyHint("force"),
                    danger: true,
                    action: () => endApp(a.key, true)
                });
            items.push({
                icon: "restart_alt",
                label: I18n.tr("Restart"),
                hint: keyHint("restart"),
                enabled: !sys || (a.unit ?? "").endsWith(".service"),
                action: () => restartApp(a.key)
            });
        }
        if (!sys) {
            items.push({
                separator: true
            });
            items.push({
                icon: "energy_savings_leaf",
                label: eff ? I18n.tr("Leave efficiency mode") : I18n.tr("Efficiency mode"),
                hint: keyHint("efficiency"),
                action: () => Pulse.setEfficiency(a.key, !eff)
            });
            items.push({
                icon: paused ? "play_arrow" : "pause",
                label: paused ? I18n.tr("Resume") : I18n.tr("Pause"),
                hint: keyHint("pause"),
                action: () => Pulse.setPaused(a.key, !paused)
            });
        }
        if (!VelaConfig.pulse.claude)
            return items;
        items.push({
            separator: true
        });
        items.push({
            icon: "",
            claude: true,
            label: I18n.tr("Ask Claude about it"),
            action: () => Pulse.askClaude("app:" + a.name)
        });
        return items;
    }

    function procMenu(p: var): var {
        if (!p)
            return [];
        const sig = (s, label, icon, danger, hint) => ({
                    icon: icon,
                    label: label,
                    danger: danger,
                    hint: hint ?? "",
                    action: () => Pulse.signalPids([p.pid], s)
                });
        return [
            {
                icon: "info_outline",
                label: I18n.tr("Details"),
                action: () => {
                    if (p.app)
                        showApp(p.app);
                    detailPid = p.pid;
                }
            },
            {
                separator: true
            },
            sig("TERM", I18n.tr("End process"), "close", false, keyHint("end")),
            sig("KILL", I18n.tr("Kill process"), "stop_circle", true, keyHint("force")),
            p.state === "T" ? sig("CONT", I18n.tr("Continue"), "play_arrow", false, keyHint("pause")) : sig("STOP", I18n.tr("Stop (pause)"), "pause", false, keyHint("pause")),
            sig("HUP", I18n.tr("Reload (SIGHUP)"), "refresh", false),
            {
                separator: true
            },
            {
                icon: "energy_savings_leaf",
                label: p.nice >= 10 ? I18n.tr("Normal priority") : I18n.tr("Low priority"),
                hint: keyHint("efficiency"),
                action: () => Pulse.renice(p.pid, p.nice >= 10 ? 0 : 19)
            }
        ];
    }

    function show(page: string): void {
        root.page = page;
    }

    function showApp(key: string): void {
        detailPid = 0;
        detailKey = key;
    }

    function showPerf(target: string): void {
        if (target === "memory" || target === "cpu" || target === "power" || target === "sensors")
            device = target;
        else if (target.startsWith("card"))
            device = "gpu:" + target;
        else if (target !== "")
            device = "disk:" + target;
        page = "performance";
    }

    function ask(title: string, text: string, confirmLabel: string, danger: bool, action: var): void {
        confirm = {
            title: title,
            text: text,
            confirmLabel: confirmLabel,
            danger: danger,
            action: action
        };
    }

    // End (SIGTERM) right away like other task managers; force quitting and
    // stopping system parts ask first.
    function endApp(key: string, force: bool): void {
        const a = Pulse.app(key);
        if (!a)
            return;
        if (force)
            ask(I18n.tr("Force %1 to quit?", a.name), I18n.tr("It gets no chance to save. Unsaved work in it is lost."), I18n.tr("Force quit"), true, () => Pulse.endApp(key, true));
        else if (a.kind === "system")
            ask(I18n.tr("Stop %1?", a.name), I18n.tr("It is part of the system. Things that depend on it may stop working until it runs again."), I18n.tr("Stop"), true, () => Pulse.endApp(key, false));
        else if (VelaConfig.pulse.confirmEnd)
            ask(I18n.tr("End %1?", a.name), I18n.tr("It is asked to close. Unsaved work may be lost."), I18n.tr("End task"), false, () => Pulse.endApp(key, false));
        else
            Pulse.endApp(key, false);
    }

    function restartApp(key: string): void {
        const a = Pulse.app(key);
        if (!a)
            return;
        if (a.kind === "window" || a.kind === "task")
            ask(I18n.tr("Restart %1?", a.name), I18n.tr("It closes and starts again. Save your work in it first."), I18n.tr("Restart"), false, () => Pulse.restartApp(key));
        else
            Pulse.restartApp(key);
    }

    function doFix(f: var): void {
        switch (f.action) {
        case "end":
            endApp(f.target, false);
            break;
        case "force":
            endApp(f.target, true);
            break;
        case "restart":
            restartApp(f.target);
            break;
        case "resume":
            Pulse.setPaused(f.target, false);
            break;
        case "efficiency":
            Pulse.setEfficiency(f.target, true);
            break;
        case "profile":
            Pulse.setProfile(f.target);
            break;
        case "reboot":
            ask(I18n.tr("Restart the computer?"), I18n.tr("All apps close. Save your work first."), I18n.tr("Restart"), true, () => Pulse.reboot());
            break;
        case "unit-restart":
            {
                const a = Pulse.app(f.target);
                if (a)
                    Pulse.restartApp(f.target);
                else
                    Pulse.unitAction(f.target, f.detail === "user", "restart");
                break;
            }
        case "unit-reset":
            Pulse.unitAction(f.target, f.detail === "user", "reset-failed");
            break;
        case "show-app":
            showApp(f.target);
            break;
        case "show-perf":
            showPerf(f.target);
            break;
        case "claude":
            Pulse.askClaude(f.target);
            break;
        }
    }

    // Window keys (Settings → Pulse → Keys) for the app whose details are
    // open, else the selected row. They act right away, without asking.
    function keyMatches(event: var, k: string): bool {
        if (!k)
            return false;
        if (k.length === 1)
            return event.text.toLowerCase() === k;
        const names = {
            Delete: Qt.Key_Delete,
            Backspace: Qt.Key_Backspace,
            Insert: Qt.Key_Insert,
            Home: Qt.Key_Home,
            End: Qt.Key_End,
            Space: Qt.Key_Space
        };
        if (names[k] !== undefined)
            return event.key === names[k];
        const f = /^F(\d+)$/.exec(k);
        return f !== null && event.key === Qt.Key_F1 + Number(f[1]) - 1;
    }

    // Defaults when the settings stream doesn't have them (older vela).
    readonly property var keys: VelaConfig.pulse.keys ?? ({
            force: "k",
            end: "g",
            restart: "r",
            pause: "p",
            efficiency: "e"
        })

    function keyHint(action: string): string {
        const k = keys[action] ?? "";
        return k.length === 1 ? k.toUpperCase() : k;
    }

    function hotkey(event: var): bool {
        if (event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier))
            return false;
        const action = ["force", "end", "restart", "pause", "efficiency"].find(a => keyMatches(event, keys[a] ?? ""));
        if (!action)
            return false;
        let app = null, proc = null;
        if (detailKey !== "")
            app = Pulse.app(detailKey);
        else if (page === "processes" && selected.startsWith("a:"))
            app = Pulse.app(selected.slice(2));
        else if (page === "processes" && selected.startsWith("p:"))
            proc = Pulse.procMap[+selected.slice(2)] ?? null;
        if (app && app.kind !== "kernel") {
            const paused = app.flags.indexOf("paused") >= 0;
            const eff = app.flags.indexOf("efficiency") >= 0;
            switch (action) {
            case "force":
                Pulse.endApp(app.key, true);
                break;
            case "end":
                Pulse.endApp(app.key, false);
                break;
            case "restart":
                Pulse.restartApp(app.key);
                break;
            case "pause":
                Pulse.setPaused(app.key, !paused);
                break;
            case "efficiency":
                Pulse.setEfficiency(app.key, !eff);
                break;
            }
            return true;
        }
        if (proc) {
            const sig = {
                force: "KILL",
                end: "TERM",
                pause: proc.state === "T" ? "CONT" : "STOP"
            }[action];
            if (sig)
                Pulse.signalPids([proc.pid], sig);
            else if (action === "efficiency")
                Pulse.renice(proc.pid, proc.nice >= 10 ? 0 : 19);
            return sig !== undefined || action === "efficiency";
        }
        return false;
    }
}
