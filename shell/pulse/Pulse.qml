pragma Singleton

import QtQuick
import Quickshell
import Quickshell.Io
import qs
import "History.js" as History

// State of the task manager, fed by `vela pulse serve` (one JSON document
// per line; commands go back as JSON lines on its stdin). Keeps the history
// of every graph, so pages can come and go without losing it.
Singleton {
    id: root

    property bool connected: false
    property var hello: ({})
    property var frame: null
    property var apps: []
    property var appMap: ({})
    property var procs: []
    property var procMap: ({})
    property var findings: []
    property int score: 100
    property var crashes: []
    property var failedUnits: []
    property bool rebootNeeded: false
    // Newest first.
    property var events: []
    property var services: []
    property bool servicesLoading: false
    property var launchable: []
    property var details: null
    // Bumped once per frame: bindings read it to re-evaluate.
    property int revision: 0
    // Seconds of history shown by the performance graphs.
    property int range: VelaConfig.pulse.rangeSecs ?? 60
    // The window's range buttons keep the choice in the settings.
    function setRange(secs: int): void {
        range = secs;
        Quickshell.execDetached([Quickshell.env("VELA_BIN") || "vela", "set", "pulse.range_secs", String(secs)]);
    }
    property int interval: 1000

    // Who asked for the process list (pages and flyouts count up/down).
    property int procWatchers: 0
    onProcWatchersChanged: send({
            cmd: "procs",
            on: procWatchers > 0
        })

    // key → action running on it ("end", "restart", …) for row spinners.
    property var busy: ({})
    // Finished actions for the toast stack: { id, ok, action, name, error }.
    property var toasts: []

    signal result(var r)
    signal frameArrived

    property int nextId: 1

    // A copy (bindings only notice new arrays); `n` = only the newest n.
    function series(key: string, n: int): var {
        root.revision;
        return History.get(key, n);
    }

    function push(key: string, v: real, keep: int): void {
        History.push(key, v, keep > 0 ? keep : Theme.pulse.history);
    }

    function app(key: string): var {
        root.revision;
        return appMap[key] ?? null;
    }

    function apply(line: string): void {
        let m;
        try {
            m = JSON.parse(line);
        } catch (e) {
            console.warn("pulse: bad line", e);
            return;
        }
        switch (m.type) {
        case "hello":
            hello = m;
            connected = true;
            if (m.interval)
                interval = m.interval;
            for (const c of queued)
                proc.write(JSON.stringify(c) + "\n");
            queued = [];
            if (procWatchers > 0)
                send({
                    cmd: "procs",
                    on: true
                });
            if (interval !== 1000)
                send({
                    cmd: "interval",
                    ms: interval
                });
            break;
        case "frame":
            onFrame(m);
            break;
        case "backlog":
            onBacklog(m);
            break;
        case "result":
            onResult(m);
            break;
        case "services":
            services = m.list;
            servicesLoading = false;
            break;
        case "apps":
            launchable = m.list;
            break;
        case "details":
            details = m;
            break;
        case "events":
            events = m.list.slice().reverse();
            break;
        }
    }

    function onFrame(f: var): void {
        push("cpu", f.cpu.usage);
        push("cpu.user", f.cpu.user);
        push("cpu.system", f.cpu.system);
        push("cpu.iowait", f.cpu.iowait);
        f.cpu.cores.forEach((c, i) => push("core." + i, c, 120));
        push("cpu.mhz", f.cpu.avgMhz);
        if (f.cpu.temp !== null && f.cpu.temp !== undefined)
            push("cpu.temp", f.cpu.temp);
        push("cpu.pressure", f.cpu.pressure);
        const mem = f.memory;
        push("mem", mem.total > 0 ? mem.used / mem.total * 100 : 0);
        push("mem.used", mem.used);
        push("mem.cache", mem.cache);
        push("swap", mem.swapTotal > 0 ? mem.swapUsed / mem.swapTotal * 100 : 0);
        push("mem.pressure", mem.pressure);
        push("io.pressure", f.io.pressureFull);
        for (const g of f.gpus) {
            push("gpu." + g.card, g.busy ?? 0);
            if (g.vramTotal)
                push("vram." + g.card, g.vramUsed / g.vramTotal * 100);
        }
        for (const d of f.disks) {
            push("disk.r." + d.name, d.readBps);
            push("disk.w." + d.name, d.writeBps);
            push("disk.busy." + d.name, d.busy);
        }
        for (const n of f.net) {
            push("net.rx." + n.iface, n.rxBps);
            push("net.tx." + n.iface, n.txBps);
        }
        const watts = f.power.batteries.reduce((s, b) => s + (b.status === "discharging" ? b.watts : 0), 0);
        push("power.watts", watts);

        const map = {};
        for (const a of f.apps) {
            map[a.key] = a;
            push("app.cpu." + a.key, a.cpu, 120);
            push("app.mem." + a.key, a.mem, 120);
        }
        History.prune(["app.cpu.", "app.mem."], map);
        if (f.procs) {
            const pm = {};
            for (const p of f.procs)
                pm[p.pid] = p;
            procs = f.procs;
            procMap = pm;
        } else if (procWatchers === 0 && procs.length > 0) {
            procs = [];
            procMap = {};
        }
        apps = f.apps;
        appMap = map;
        findings = f.findings;
        score = f.score;
        crashes = f.crashes;
        failedUnits = f.failedUnits;
        rebootNeeded = f.rebootNeeded;
        if (f.events.length > 0)
            events = f.events.slice().reverse().concat(events).slice(0, 300);
        frame = f;
        revision++;
        frameArrived();
    }

    // The daemon's background recording: drawn as if the window had been
    // open (each point repeated to the window's interval), plus its events.
    function onBacklog(b: var): void {
        const ageMs = Date.now() - b.t;
        if (ageMs > 10 * 60 * 1000)
            return;
        const rep = Math.max(1, Math.round(b.interval / interval));
        for (const key in b.series) {
            const keep = key.startsWith("core.") ? 120 : Theme.pulse.history;
            const vals = b.series[key].slice(-Math.ceil(keep / rep));
            for (const v of vals)
                for (let i = 0; i < rep; i++)
                    push(key, v, keep);
        }
        if (b.events.length > 0)
            events = b.events.slice().reverse().slice(0, 300);
        revision++;
    }

    function onResult(r: var): void {
        const b = Object.assign({}, busy);
        if (r.key)
            delete b[r.key];
        busy = b;
        if (r.action !== "focus" && (r.action !== "claude" || !r.ok))
            toasts = toasts.concat([r]).slice(-4);
        result(r);
    }

    function dismissToast(id: int): void {
        toasts = toasts.filter(t => t.id !== id);
    }

    // Commands before the backend said hello wait here.
    property var queued: []

    function send(cmd: var): int {
        const id = nextId++;
        cmd.id = id;
        if (proc.running && connected)
            proc.write(JSON.stringify(cmd) + "\n");
        else
            queued = queued.concat([cmd]);
        return id;
    }

    function act(key: string, action: string, cmd: var): int {
        const b = Object.assign({}, busy);
        b[key] = action;
        busy = b;
        cmd.key = key;
        return send(cmd);
    }

    function endApp(key: string, force: bool): int {
        return act(key, force ? "force" : "end", {
            cmd: "end",
            force: force
        });
    }
    function restartApp(key: string): int {
        return act(key, "restart", {
            cmd: "restart"
        });
    }
    function setEfficiency(key: string, on: bool): int {
        return act(key, "efficiency", {
            cmd: "efficiency",
            on: on
        });
    }
    function setPaused(key: string, on: bool): int {
        return act(key, on ? "pause" : "resume", {
            cmd: "pause",
            on: on
        });
    }
    function signalPids(pids: var, sig: string): int {
        return send({
            cmd: "signal",
            pids: pids,
            signal: sig
        });
    }
    function renice(pid: int, nice: int): int {
        return send({
            cmd: "renice",
            pid: pid,
            nice: nice
        });
    }
    function unitAction(unit: string, user: bool, action: string): int {
        return act(unit, "unit-" + action, {
            cmd: "unit",
            unit: unit,
            user: user,
            action: action
        });
    }
    function setProfile(p: string): int {
        return send({
            cmd: "profile",
            profile: p
        });
    }
    function run(command: string, terminal: bool): int {
        return send({
            cmd: "run",
            command: command,
            terminal: terminal
        });
    }
    function launch(desktopId: string): int {
        return send({
            cmd: "launch",
            desktopId: desktopId
        });
    }
    function focus(address: string): int {
        return send({
            cmd: "focus",
            address: address
        });
    }
    function askClaude(topic: string): int {
        return send({
            cmd: "claude",
            topic: topic
        });
    }
    function reboot(): int {
        return send({
            cmd: "reboot"
        });
    }
    function loadServices(): void {
        servicesLoading = true;
        send({
            cmd: "services"
        });
    }
    function loadDetails(pid: int): void {
        send({
            cmd: "details",
            pid: pid
        });
    }
    function loadLaunchable(): void {
        if (launchable.length === 0)
            send({
                cmd: "apps"
            });
    }
    function setSampleInterval(ms: int): void {
        interval = ms;
        send({
            cmd: "interval",
            ms: ms
        });
    }

    Process {
        id: proc

        command: [Quickshell.env("VELA_BIN") || "vela", "pulse", "serve"]
        running: true
        stdinEnabled: true
        stdout: SplitParser {
            onRead: line => root.apply(line)
        }
        stderr: SplitParser {
            onRead: line => console.warn("pulse:", line)
        }
        onExited: code => {
            root.connected = false;
            console.warn("pulse: backend exited with", code);
            retry.start();
        }
    }

    Timer {
        id: retry

        interval: 1500
        onTriggered: proc.running = true
    }
}
