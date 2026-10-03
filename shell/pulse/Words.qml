pragma Singleton

import QtQuick
import Quickshell
import qs
import "Fmt.js" as Fmt

// Every sentence Pulse shows, built from the backend's kinds and values.
Singleton {
    id: root

    readonly property var loc: Qt.locale()

    function bytes(v: real): string {
        return Fmt.bytes(v, loc);
    }
    function pct(v: real): string {
        return Fmt.percent(v, loc);
    }

    function kind(k: string): string {
        return {
            window: I18n.tr("Apps"),
            background: I18n.tr("Background apps"),
            task: I18n.tr("Terminal tasks"),
            service: I18n.tr("Services"),
            system: I18n.tr("System"),
            kernel: I18n.tr("Kernel")
        }[k] ?? k;
    }

    function kindOne(k: string): string {
        return {
            window: I18n.tr("App"),
            background: I18n.tr("Background app"),
            task: I18n.tr("Terminal task"),
            service: I18n.tr("User service"),
            system: I18n.tr("System"),
            kernel: I18n.tr("Kernel")
        }[k] ?? k;
    }

    function flag(f: string): string {
        return {
            "not-responding": I18n.tr("Not responding"),
            "hung": I18n.tr("Stuck"),
            "crashed": I18n.tr("Crashed"),
            "runaway": I18n.tr("High CPU"),
            "leak": I18n.tr("Memory growing"),
            "needs-restart": I18n.tr("Needs restart"),
            "paused": I18n.tr("Paused"),
            "zombies": I18n.tr("Zombies"),
            "efficiency": I18n.tr("Efficiency mode")
        }[f] ?? f;
    }

    function flagExplain(f: string): string {
        return {
            "not-responding": I18n.tr("Its window stopped answering Hyprland. Wait a moment, or force it to quit and start it again."),
            "hung": I18n.tr("It waits for a device that doesn't answer (a slow disk, a network share or a driver)."),
            "crashed": I18n.tr("It crashed within the last hour. The crash report is in coredumpctl."),
            "runaway": I18n.tr("It keeps at least one core busy for over a minute without being in front — maybe stuck in a loop."),
            "leak": I18n.tr("Its memory grows steadily. A restart frees it."),
            "needs-restart": I18n.tr("It was updated while running and still uses the old files. Restart it to load the new version."),
            "paused": I18n.tr("It is paused and does nothing until it's resumed."),
            "zombies": I18n.tr("It doesn't clean up its finished child processes."),
            "efficiency": I18n.tr("It only gets CPU and disk time nobody else needs.")
        }[f] ?? "";
    }

    function flagIcon(f: string): string {
        return {
            "not-responding": "error",
            "hung": "schedule",
            "crashed": "error",
            "runaway": "pulse_cpu",
            "leak": "pulse_memory",
            "needs-restart": "restart_alt",
            "paused": "pause",
            "zombies": "warning",
            "efficiency": "energy_savings_leaf"
        }[f] ?? "info";
    }

    function flagColor(f: string): color {
        switch (f) {
        case "not-responding":
        case "hung":
        case "crashed":
            return Theme.pulse.crit;
        case "efficiency":
            return Theme.pulse.efficiency;
        case "paused":
            return Theme.colors.textMuted;
        default:
            return Theme.pulse.warn;
        }
    }

    function severityColor(s: string): color {
        return s === "critical" ? Theme.pulse.crit : s === "warning" ? Theme.pulse.warn : Theme.colors.primary;
    }

    function severityIcon(s: string): string {
        return s === "critical" ? "error" : s === "warning" ? "warning" : "info";
    }

    // app-onlyoffice\x2ddesktopeditors@7573….service → app-onlyoffice-desktopeditors
    function unitName(u: string): string {
        return (u ?? "").replace(/\\x([0-9a-fA-F]{2})/g, (m, h) => String.fromCharCode(parseInt(h, 16))).replace(/@[0-9a-f]{16,}/, "").replace(/\.service$/, "");
    }

    function topName(v: var): string {
        return v.top && v.top.length > 0 ? v.top[0].name : "";
    }

    function names(list: var): string {
        const n = (list ?? []).map(x => x.name ?? x);
        if (n.length <= 3)
            return n.join(", ");
        return I18n.tr("%1 and %2 more", n.slice(0, 3).join(", "), n.length - 3);
    }

    function title(f: var): string {
        const v = f.values;
        switch (f.kind) {
        case "not-responding":
            return I18n.tr("%1 is not responding", v.name);
        case "hung":
            return v.count === 1 ? I18n.tr("A process is stuck waiting") : I18n.tr("%1 processes are stuck waiting", v.count);
        case "cpu-busy":
            return I18n.tr("The processor is overloaded");
        case "memory-low":
            return I18n.tr("Memory is running low");
        case "swapping":
            return I18n.tr("The system is swapping");
        case "io-busy":
            return I18n.tr("Programs are waiting for the disk");
        case "cpu-hot":
            return I18n.tr("The processor is very hot");
        case "cpu-throttling":
            return I18n.tr("The processor slows down because of heat");
        case "power-saver-slow":
            return I18n.tr("Power saver mode slows things down");
        case "battery-drain":
            return I18n.tr("The battery drains fast");
        case "dgpu-awake":
            return I18n.tr("The graphics card is awake on battery");
        case "battery-worn":
            return I18n.tr("The battery has aged");
        case "gpu-busy":
            return I18n.tr("%1 is fully loaded", v.gpu);
        case "disk-full":
            return I18n.tr("%1 is almost full", v.path);
        case "runaway":
            return I18n.tr("%1 has been using a lot of CPU for a long time", v.name);
        case "memory-leak":
            return I18n.tr("%1 keeps using more memory", v.name);
        case "keeps-crashing":
            return I18n.tr("%1 keeps crashing", v.name);
        case "unit-failed":
            return I18n.tr("%1 failed", v.description || unitName(v.unit));
        case "needs-restart":
            return v.count === 1 ? I18n.tr("A program runs outdated code") : I18n.tr("%1 programs run outdated code", v.count);
        case "reboot-kernel":
            return I18n.tr("Restart to finish the update");
        case "paused":
            return I18n.tr("Paused apps");
        case "zombies":
            return I18n.tr("%1 zombie processes", v.count);
        case "compositor-busy":
            return I18n.tr("Hyprland works hard");
        }
        return f.kind;
    }

    function text(f: var): string {
        const v = f.values;
        const top = topName(v);
        switch (f.kind) {
        case "not-responding":
            return I18n.tr("Its window stopped answering. Wait a moment, or force it to quit and start it again.");
        case "hung":
            return I18n.tr("%1 waits for a device that doesn't answer — usually a slow disk, a network share or a driver. Ending it often only works once the device answers again.", names(v.names));
        case "cpu-busy":
            return I18n.tr("Programs wait %1 of the time for the processor.", pct(v.pressure)) + (top ? " " + I18n.tr("%1 uses the most (%2).", top, pct(v.top[0].value)) : "");
        case "memory-low":
            return I18n.tr("Only %1 of %2 are still free.", bytes(v.available), bytes(v.total)) + (top ? " " + I18n.tr("%1 uses the most (%2) — closing it frees memory right away.", top, bytes(v.top[0].value)) : "");
        case "swapping":
            return I18n.tr("Memory moves to and from swap at %1. That makes everything sluggish; close memory-hungry apps.", Fmt.rate(v["in"] + v.out, loc));
        case "io-busy":
            {
                let t = I18n.tr("For %1 of the time all programs are waiting for input/output.", pct(v.pressure));
                if (v.blocked && v.blocked.length > 0)
                    t += " " + I18n.tr("Waiting right now: %1.", names(v.blocked));
                if (top)
                    t += " " + I18n.tr("Most disk activity: %1.", top);
                else if (v.swapUsed > 0)
                    t += " " + I18n.tr("There is little visible disk traffic — that's often swap (%1 in use) or a slow network drive.", bytes(v.swapUsed));
                return t;
            }
        case "cpu-hot":
            return I18n.tr("%1 — close to its limit of %2. It slows itself down to cool off.", Fmt.celsius(v.temp, loc), Fmt.celsius(v.crit, loc));
        case "cpu-throttling":
            return I18n.tr("It throttled %1 times in the last 30 seconds at %2. Free the vents or lower the load.", v.events, Fmt.celsius(v.temp, loc));
        case "power-saver-slow":
            return I18n.tr("You're plugged in, but the power profile is “Power saver”, which holds the processor back.");
        case "battery-drain":
            return I18n.tr("The system draws %1, so the battery won't last long.", Fmt.watts(v.watts, loc)) + (top ? " " + I18n.tr("%1 uses the most CPU.", top) : "");
        case "dgpu-awake":
            return v.apps && v.apps.length > 0 ? I18n.tr("%1 uses power because %2 keeps it awake.", v.gpu, names(v.apps)) : I18n.tr("%1 uses power although nothing seems to need it.", v.gpu);
        case "battery-worn":
            return I18n.tr("It holds %1 of its original capacity (%2 charge cycles).", pct(v.health), v.cycles);
        case "gpu-busy":
            return top ? I18n.tr("%1 uses it the most.", top) : I18n.tr("Games and video exports do that; otherwise check what's running.");
        case "disk-full":
            return I18n.tr("Only %1 of %2 left. Programs can fail when it runs out.", bytes(v.free), bytes(v.total));
        case "runaway":
            return I18n.tr("It keeps %1 cores busy for over a minute without being in front — possibly stuck in a loop.", Fmt.num(v.cores, 1, loc));
        case "memory-leak":
            return I18n.tr("It grew by %1 per minute for %2 minutes and now uses %3. A restart frees it.", bytes(v.perMinute), Math.round(v.minutes), bytes(v.mem));
        case "keeps-crashing":
            return I18n.tr("%1 crashes in the last 24 hours (%2). Claude can read the crash reports and find the cause.", v.count, v.signal);
        case "unit-failed":
            return I18n.tr("The %1 service %2 stopped with an error.", v.user ? I18n.tr("user") : I18n.tr("system"), unitName(v.unit));
        case "needs-restart":
            return I18n.tr("Updated while running, they still use the old files: %1. Restart them — or the computer — to load the new versions.", names(v.apps));
        case "reboot-kernel":
            return I18n.tr("A new kernel was installed. Until you restart, the running one is outdated and new drivers may not load.");
        case "paused":
            return I18n.tr("%1 is paused and does nothing until it's resumed.", names(v.apps));
        case "zombies":
            return v.apps && v.apps.length > 0 ? I18n.tr("Finished processes %1 never cleaned up. Restarting it removes them.", names(v.apps)) : I18n.tr("Finished processes that their parent never cleaned up.");
        case "compositor-busy":
            return I18n.tr("The compositor keeps %1 cores busy. Blur, shadows and animations on many screens cost CPU and GPU.", Fmt.num(v.cores, 1, loc));
        }
        return JSON.stringify(v);
    }

    function fix(x: var): string {
        switch (x.action) {
        case "end":
            return I18n.tr("End task");
        case "force":
            return I18n.tr("Force quit");
        case "restart":
            {
                const a = Pulse.app(x.target);
                return a ? I18n.tr("Restart %1", a.name) : I18n.tr("Restart");
            }
        case "resume":
            {
                const a = Pulse.app(x.target);
                return a ? I18n.tr("Resume %1", a.name) : I18n.tr("Resume");
            }
        case "efficiency":
            return I18n.tr("Efficiency mode");
        case "profile":
            return I18n.tr("Use %1", profile(x.target));
        case "reboot":
            return I18n.tr("Restart computer");
        case "unit-restart":
            return I18n.tr("Restart service");
        case "unit-reset":
            return I18n.tr("Clear error");
        case "show-app":
            {
                const a = Pulse.app(x.target);
                return a ? I18n.tr("Show %1", a.name) : I18n.tr("Show");
            }
        case "show-perf":
            return I18n.tr("Details");
        case "claude":
            return "Claude";
        }
        return x.action;
    }

    function fixIcon(action: string): string {
        return {
            end: "close",
            force: "stop_circle",
            restart: "restart_alt",
            resume: "play_arrow",
            efficiency: "energy_savings_leaf",
            profile: "speed",
            reboot: "reboot",
            "unit-restart": "restart_alt",
            "unit-reset": "check",
            "show-app": "chevron_right",
            "show-perf": "chevron_right",
            claude: ""
        }[action] ?? "";
    }

    function profile(p: string): string {
        return {
            "power-saver": I18n.tr("Power saver"),
            balanced: I18n.tr("Balanced"),
            performance: I18n.tr("Performance")
        }[p] ?? p;
    }

    // Seconds in an event's detail as a short duration.
    function dur(secs: string): string {
        const s = Number(secs);
        if (!secs || isNaN(s))
            return "";
        if (s < 60)
            return I18n.tr("%1 s", s);
        return I18n.duration(s);
    }

    function event(e: var): string {
        const d = dur(e.detail);
        switch (e.kind) {
        case "task-started":
            return I18n.tr("%1 started in a terminal", e.name);
        case "task-finished":
            return d ? I18n.tr("%1 finished after %2", e.name, d) : I18n.tr("%1 finished", e.name);
        case "service-started":
            return I18n.tr("Service %1 started", e.name);
        case "service-stopped":
            return I18n.tr("Service %1 stopped", e.name);
        case "cpu-busy":
            return e.name ? I18n.tr("High CPU load, mostly %1", e.name) : I18n.tr("High CPU load");
        case "cpu-calm":
            return I18n.tr("CPU load back to normal after %1", d);
        case "memory-pressure":
            return e.name ? I18n.tr("Memory got tight, mostly %1", e.name) : I18n.tr("Memory got tight");
        case "memory-ok":
            return I18n.tr("Memory relaxed after %1", d);
        case "io-wait":
            return e.name ? I18n.tr("Programs wait for the disk, mostly %1", e.name) : I18n.tr("Programs wait for the disk");
        case "io-ok":
            return I18n.tr("Disk waits over after %1", d);
        case "hot":
            return I18n.tr("Processor at %1 °C", e.name);
        case "cool":
            return I18n.tr("Processor cooled down after %1", d);
        case "throttling":
            return I18n.tr("Processor throttles because of heat");
        case "throttling-over":
            return I18n.tr("Throttling over after %1", d);
        case "ac-on":
            return I18n.tr("Plugged in");
        case "ac-off":
            return e.name ? I18n.tr("On battery (%1 %)", e.name) : I18n.tr("On battery");
        case "battery-low":
            return I18n.tr("Battery at %1 %", e.name);
        case "net-up":
            return I18n.tr("%1 connected", e.name);
        case "net-down":
            return I18n.tr("%1 disconnected", e.name);
        case "mounted":
            return I18n.tr("%1 mounted", e.name);
        case "unmounted":
            return I18n.tr("%1 unmounted", e.name);
        case "profile":
            return I18n.tr("Power profile: %1", profile(e.name));
        case "gpu-awake":
            return I18n.tr("%1 woke up", e.name);
        case "gpu-asleep":
            return I18n.tr("%1 went to sleep", e.name);
        case "resumed":
            return I18n.tr("Woke up after %1", d);
        case "boot":
            return I18n.tr("Computer started");
        case "started":
            return I18n.tr("%1 started", e.name);
        case "closed":
            return I18n.tr("%1 closed", e.name);
        case "crashed":
            return I18n.tr("%1 crashed", e.name);
        case "oom":
            return e.name ? I18n.tr("Out of memory: %1 was ended", e.name) : I18n.tr("Out of memory: a process was ended");
        case "failed":
            return I18n.tr("%1 failed", e.name);
        case "not-responding":
            return I18n.tr("%1 stopped responding", e.name);
        case "responding":
            return I18n.tr("%1 responds again", e.name);
        case "ended":
            return I18n.tr("%1 ended", e.name);
        case "restarted":
            return I18n.tr("%1 restarted", e.name);
        case "action-failed":
            return I18n.tr("%1: %2", e.name, e.detail);
        }
        return e.name;
    }

    function eventColor(kind: string): color {
        switch (kind) {
        case "crashed":
        case "oom":
        case "failed":
        case "not-responding":
        case "action-failed":
        case "hot":
        case "battery-low":
            return Theme.pulse.crit;
        case "cpu-busy":
        case "memory-pressure":
        case "io-wait":
        case "throttling":
            return Theme.pulse.warn;
        case "started":
        case "responding":
        case "restarted":
        case "task-started":
        case "service-started":
        case "cpu-calm":
        case "memory-ok":
        case "io-ok":
        case "cool":
        case "throttling-over":
        case "net-up":
        case "ac-on":
        case "boot":
        case "resumed":
            return Theme.pulse.ok;
        case "mounted":
        case "unmounted":
        case "profile":
        case "gpu-awake":
        case "gpu-asleep":
            return Theme.colors.primary;
        default:
            return Theme.colors.textMuted;
        }
    }

    function eventIcon(kind: string): string {
        return {
            started: "play_arrow",
            closed: "close",
            crashed: "error",
            oom: "pulse_memory",
            failed: "error",
            "not-responding": "schedule",
            responding: "check",
            ended: "stop_circle",
            restarted: "restart_alt",
            "action-failed": "warning",
            "task-started": "terminal",
            "task-finished": "check",
            "service-started": "services",
            "service-stopped": "services",
            "cpu-busy": "pulse_cpu",
            "cpu-calm": "pulse_cpu",
            "memory-pressure": "pulse_memory",
            "memory-ok": "pulse_memory",
            "io-wait": "storage",
            "io-ok": "storage",
            hot: "pulse_thermo",
            cool: "pulse_thermo",
            throttling: "pulse_thermo",
            "throttling-over": "pulse_thermo",
            "ac-on": "battery_charging_full",
            "ac-off": "battery_full",
            "battery-low": "battery_alert",
            "net-up": "wifi",
            "net-down": "signal_wifi_off",
            mounted: "storage",
            unmounted: "storage",
            profile: "speed",
            "gpu-awake": "pulse_gpu",
            "gpu-asleep": "pulse_gpu",
            resumed: "bedtime_off",
            boot: "power_settings_new"
        }[kind] ?? "info";
    }

    function result(r: var): string {
        if (!r.ok)
            return I18n.tr("Didn't work: %1", r.error || r.action);
        const n = r.name;
        switch (r.action) {
        case "end":
            return I18n.tr("%1 ended", n);
        case "force":
            return I18n.tr("%1 was forced to quit", n);
        case "restart":
            return I18n.tr("%1 restarted", n);
        case "efficiency-on":
            return I18n.tr("%1 runs in efficiency mode", n);
        case "efficiency-off":
            return I18n.tr("%1 runs normally again", n);
        case "pause":
            return I18n.tr("%1 paused", n);
        case "resume":
            return I18n.tr("%1 resumed", n);
        case "signal":
            return I18n.tr("Signal sent: %1", n);
        case "renice":
            return I18n.tr("Priority changed");
        case "unit-start":
            return I18n.tr("%1 started", n);
        case "unit-stop":
            return I18n.tr("%1 stopped", n);
        case "unit-restart":
            return I18n.tr("%1 restarted", n);
        case "unit-reset-failed":
            return I18n.tr("Error of %1 cleared", n);
        case "unit-enable":
            return I18n.tr("%1 enabled", n);
        case "unit-disable":
            return I18n.tr("%1 disabled", n);
        case "profile":
            return I18n.tr("Power profile: %1", profile(n));
        case "run":
        case "launch":
            return I18n.tr("Started");
        case "claude":
            return I18n.tr("Claude is on it");
        case "reboot":
            return I18n.tr("Restarting…");
        }
        return I18n.tr("Done");
    }

    function stateName(s: string): string {
        return {
            R: I18n.tr("Running"),
            S: I18n.tr("Sleeping"),
            D: I18n.tr("Waiting for I/O"),
            Z: I18n.tr("Zombie"),
            T: I18n.tr("Stopped"),
            t: I18n.tr("Traced"),
            I: I18n.tr("Idle"),
            X: I18n.tr("Dead")
        }[s] ?? s;
    }

    function ago(unixSeconds: real): string {
        const s = Date.now() / 1000 - unixSeconds;
        if (s < 60)
            return I18n.tr("just now");
        if (s < 3600)
            return I18n.tr("%1 min ago", Math.floor(s / 60));
        if (s < 86400)
            return I18n.tr("%1 h ago", Math.floor(s / 3600));
        return I18n.tr("%1 d ago", Math.floor(s / 86400));
    }

    function since(unixSeconds: real): string {
        const s = Math.max(0, Date.now() / 1000 - unixSeconds);
        const d = Math.floor(s / 86400);
        const h = Math.floor(s % 86400 / 3600);
        const m = Math.floor(s % 3600 / 60);
        if (d > 0)
            return I18n.tr("%1 d %2 h", d, h);
        if (h > 0)
            return I18n.tr("%1 h %2 min", h, m);
        return I18n.tr("%1 min", Math.max(m, 0));
    }
}
