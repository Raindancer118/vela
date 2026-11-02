// Drives: formatting, the words for drives and jobs (each with a German
// translation), the backend's hello through `vela drives serve`, and the
// window loading without errors.
//   qs -p test-drives.qml   (scripts/shell-test.sh, VELA_BIN = vela); prints PASS or FAIL
import QtQuick
import Quickshell
import qs
import qs.drives
import "drives/Words.js" as W

ShellRoot {
    id: root

    property var errors: []

    function check(ok: bool, name: string): void {
        if (!ok)
            errors.push(name);
    }

    function finish(): void {
        console.log(errors.length === 0 ? "PASS drives" : "FAIL drives: " + errors.join("; "));
        Qt.quit();
    }

    // Every string of a function's result must have a German version.
    function german(text: string): bool {
        return I18n.translations.de[text] !== undefined;
    }

    Component.onCompleted: {
        const en = Qt.locale("en_US");
        check(W.size(15581839360, en) === "15.6 GB" && W.size(0, en) === "0 B" && W.size(21e6, en) === "21.0 MB", "size");
        check(W.nominal(15581839360, en) === "16 GB" && W.nominal(62.5e9, en) === "64 GB" && W.nominal(268435456, en) === "268 MB", "nominal size");
        check(W.monogram("Pop!_OS") === "Po" && W.monogram("") === "?", "monogram");
        check(W.logoNames("arch")[0] === "distributor-logo-archlinux" && W.logoNames("nope").length === 0, "logo names");
        check(W.bundledLogo("proxmox") === "proxmox" && W.bundledLogo("nope") === "", "bundled logos");
        check(W.familyColor("ubuntu", false) !== "" && W.familyColor("nope", false) === "", "family colours");
        check(W.left(90, (s, ...a) => s.replace("%1", a[0])) === "2 min left", "time left");

        const stick = {
            id: "/b/sda",
            device: "/dev/sda",
            name: "",
            kind: "usb",
            size: 16e9,
            volumes: [{ id: "/b/sda1", label: "STICK", fs: "vfat", mount_points: ["/run/media/x/STICK"] }]
        };
        check(DrivesText.name(stick) === I18n.tr("USB stick"), "nameless stick");
        check(DrivesText.contents(stick) === "STICK · FAT32", "contents: " + DrivesText.contents(stick));
        check(DrivesText.mounted(stick), "mounted");
        const ventoy = Object.assign({}, stick, { ventoy: { mount: "/m", files: [{}, {}], free: 1 } });
        check(DrivesText.contents(ventoy) === I18n.tr("Ventoy · %1 images", 2), "ventoy contents");
        check(DrivesText.contents(Object.assign({}, stick, { volumes: [] })) === I18n.tr("Empty"), "empty stick");

        // Every phase and failure has words, all translated.
        const sample = { done: 5e8, total: 1e9, rate: 2e7 };
        for (const phase of ["start", "unmount", "auth", "ventoy", "write", "verify", "finish", "erase", "format", "mount", "copy", "cancelled", "failed"]) {
            const t = DrivesText.phase(Object.assign({ phase: phase, kind: "write" }, sample));
            check(t !== "" && german(t), "phase " + phase + ": " + t);
        }
        for (const kind of ["write", "format", "eject", "add", "remove"]) {
            const t = DrivesText.phase({ phase: "done", kind: kind });
            check(german(t), "done " + kind + ": " + t);
        }
        for (const reason of ["auth", "busy", "gone", "space", "verify", "unpack", "readonly", "running", "backend"]) {
            const t = DrivesText.detail({ phase: "failed", reason: reason });
            check(t !== "" && german(t), "reason " + reason + ": " + t);
        }
        check(DrivesText.progressLine(sample).startsWith(I18n.tr("%1 of %2", W.size(5e8, Qt.locale()), W.size(1e9, Qt.locale()))), "progress line");

        // Opening request before the drives are known: picked once they arrive.
        Drives.open({ tab: "format", drive: "/dev/sdz", images: [] });
        check(Drives.tab === "format" && Drives.wantedDevice === "/dev/sdz", "open request");
        Drives.apply(JSON.stringify({ type: "drives", drives: [Object.assign({}, stick, { id: "/b/sdz", device: "/dev/sdz" })] }));
        check(Drives.driveId === "/b/sdz" && Drives.wantedDevice === "", "wanted drive picked");
        check(Drives.fits({ written: 20e9, size: 1 }, Drives.drive) === false && Drives.fits({ written: 0, size: 1 }, Drives.drive), "fits");

        const w = windowComponent.createObject(root, { visible: false });
        check(w !== null, "window loads");
        timeout.start();
    }

    Component {
        id: windowComponent

        DrivesWindow {}
    }

    // The real backend says hello.
    Timer {
        interval: 200
        repeat: true
        running: true
        onTriggered: if (Drives.connected) {
            root.check((Drives.hello.formats?.fat32 !== undefined) || Drives.hello.formats !== undefined, "hello with formats");
            root.check(typeof Drives.hello.ventoy === "boolean", "hello says whether Ventoy is there");
            root.finish();
        }
    }

    Timer {
        id: timeout

        interval: 15000
        onTriggered: {
            root.check(false, "backend never said hello");
            root.finish();
        }
    }
}
