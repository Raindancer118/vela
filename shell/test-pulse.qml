// Pulse test: qs -p test-pulse.qml (VELA_BIN = vela); prints PASS or FAIL.
// Formatting, list syncing and every finding text without the backend;
// then frames from `vela pulse serve` and every page loading with them.
import QtQuick
import Quickshell
import qs
import qs.pulse
import "pulse/Fmt.js" as Fmt
import "pulse/Model.js" as Model

ShellRoot {
    id: root

    property var errors: []
    property int frames: 0

    function expect(name: string, ok: bool): void {
        if (!ok)
            errors.push(name);
    }

    function finish(): void {
        console.log(errors.length === 0 ? "PASS pulse" : "FAIL pulse: " + errors.join("; "));
        Qt.quit();
    }

    ListModel {
        id: model
    }

    Component.onCompleted: {
        const en = Qt.locale("en_US");
        expect("bytes", Fmt.bytes(1536, en) === "1.50 KB" && Fmt.bytes(0, en) === "0 B" && Fmt.bytes(3 * 1024 * 1024 * 1024, en) === "3.00 GB");
        expect("bits", Fmt.bits(125000, en) === "1.0 Mbit/s");
        expect("percent", Fmt.percent(5.25, en) === "5.3 %" && Fmt.percent(50, en) === "50 %");
        expect("niceMax", Fmt.niceMax([0.3, 7.2], 1) === 8 && Fmt.niceMax([], 1) === 1);
        expect("clock", Fmt.clock(3725) === "1:02:05");
        Model.sync(model, ["a", "b", "c"]);
        Model.sync(model, ["c", "a", "d"]);
        const keys = [];
        for (let i = 0; i < model.count; i++)
            keys.push(model.get(i).key);
        expect("sync " + keys, keys.join() === "c,a,d");
        expect("matches", Model.matches("fire fox", ["Firefox", "fox"]) && !Model.matches("chrome", ["Firefox"]));
        expect("score", Model.score("fi", "Firefox") > Model.score("fi", "Nautilus files"));
        // Every finding kind gets real words (not the kind or JSON).
        const v = {
            name: "App",
            count: 3,
            names: ["a"],
            usage: 95,
            pressure: 40,
            top: [{
                    name: "X",
                    value: 50
                }],
            available: 1,
            total: 2,
            "in": 1,
            out: 1,
            temp: 99,
            crit: 100,
            events: 2,
            watts: 30,
            gpu: "GPU",
            apps: [{
                    name: "Y"
                }],
            health: 60,
            cycles: 9,
            path: "/",
            free: 1,
            cores: 1.2,
            perMinute: 1e6,
            minutes: 10,
            mem: 1e9,
            signal: "SIGSEGV",
            unit: "x.service",
            description: "X",
            user: true,
            blocked: ["kworker"],
            swapUsed: 5
        };
        for (const k of ["not-responding", "hung", "cpu-busy", "memory-low", "swapping", "io-busy", "cpu-hot", "cpu-throttling", "power-saver-slow", "battery-drain", "dgpu-awake", "battery-worn", "gpu-busy", "disk-full", "runaway", "memory-leak", "keeps-crashing", "unit-failed", "needs-restart", "reboot-kernel", "paused", "zombies", "compositor-busy"]) {
            const f = {
                kind: k,
                values: v,
                severity: "warning"
            };
            const t = Words.title(f), x = Words.text(f);
            expect("words " + k, t !== k && t !== "" && x !== "" && x.indexOf("{") < 0);
        }
        expect("unit name", Words.unitName("app-onlyoffice\\x2ddesktopeditors@75734ef238da49ffb092e917ba1ecc42.service") === "app-onlyoffice-desktopeditors");
    }

    // Pages are Items: they load without a window.
    Item {
        id: host

        width: 1100
        height: 760

        Repeater {
            id: pages

            model: Pulse.frame ? PulseUi.pages : []

            Loader {
                required property string modelData

                anchors.fill: parent
                source: "pulse/pages/" + modelData.charAt(0).toUpperCase() + modelData.slice(1) + "Page.qml"
            }
        }
    }

    Connections {
        target: Pulse

        function onFrameArrived(): void {
            root.frames++;
            if (root.frames < 3)
                return;
            root.expect("connected", Pulse.connected && Pulse.apps.length > 0);
            root.expect("history", Pulse.series("cpu").length >= 3);
            root.expect("memory", Pulse.frame.memory.total > 0);
            for (let i = 0; i < pages.count; i++) {
                const l = pages.itemAt(i);
                root.expect("page " + l.modelData, l.status === Loader.Ready);
            }
            root.finish();
        }
    }

    Timer {
        running: true
        interval: 15000
        onTriggered: {
            root.expect("no frames (connected=" + Pulse.connected + ")", false);
            root.finish();
        }
    }
}
