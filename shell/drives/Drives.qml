pragma Singleton

import QtQuick
import Quickshell
import Quickshell.Io

// State of the Drives window, fed by `vela drives serve` (JSON lines; the
// commands go back on its stdin).
Singleton {
    id: root

    property bool connected: false
    property var hello: ({})
    property var drives: []
    property var images: []
    // The running or last job: { id, kind, phase, done, total, rate, reason, … }.
    property var job: null
    readonly property bool busy: job !== null && !["done", "failed", "cancelled"].includes(job.phase)

    property string tab: "image"
    property string driveId: ""
    property string imagePath: ""
    // What `vela drives` asked for before the drives were listed.
    property string wantedDevice: ""

    readonly property var drive: drives.find(d => d.id === driveId) ?? null
    readonly property var image: images.find(i => i.path === imagePath) ?? null

    property int nextId: 1
    property var queued: []

    function send(cmd: var): void {
        if (!connected) {
            queued.push(cmd);
            return;
        }
        proc.write(JSON.stringify(cmd) + "\n");
    }

    // { tab, drive: "/dev/sda", images: [paths] } from `vela drives`.
    function open(request: var): void {
        if (!request)
            return;
        if (request.tab === "image" || request.tab === "format")
            tab = request.tab;
        if (request.images?.length > 0) {
            send({
                cmd: "extra",
                paths: request.images
            });
            imagePath = request.images[0];
        }
        if (request.drive) {
            wantedDevice = request.drive;
            pickWanted();
        }
    }

    function pickWanted(): void {
        const d = drives.find(d => d.device === wantedDevice);
        if (d) {
            driveId = d.id;
            wantedDevice = "";
        }
    }

    function start(cmd: var): void {
        cmd.id = nextId++;
        job = {
            id: cmd.id,
            kind: cmd.cmd,
            drive: drive?.id ?? null,
            phase: "start"
        };
        send(cmd);
    }

    // On a Ventoy stick the image is copied next to the others.
    function write(verify: bool): void {
        if (drive && image)
            start({
                cmd: "write",
                drive: drive.id,
                image: image.path,
                verify: verify
            });
    }

    function format(fs: string, label: string, erase: bool, ventoy: bool): void {
        if (drive)
            start({
                cmd: "format",
                drive: drive.id,
                fs: fs,
                label: label,
                erase: erase,
                ventoy: ventoy
            });
    }

    // An image on a Ventoy stick (path relative to its partition).
    function remove(path: string): void {
        if (drive)
            start({
                cmd: "remove",
                drive: drive.id,
                path: path
            });
    }

    function eject(): void {
        if (drive)
            start({
                cmd: "eject",
                drive: drive.id
            });
    }

    function keep(path: string): void {
        start({
            cmd: "add",
            path: path
        });
    }

    function cancel(): void {
        send({
            cmd: "cancel"
        });
    }

    function dismissJob(): void {
        if (!busy)
            job = null;
    }

    // An image fits when its size is known and not above the drive's (on a
    // Ventoy stick: its free space; the file is copied as it is).
    function fits(img: var, d: var): bool {
        if (!img || !d)
            return true;
        if (d.ventoy)
            return !d.ventoy.mount || img.size <= d.ventoy.free;
        return img.written === 0 || img.written <= d.size;
    }

    function apply(line: string): void {
        let m;
        try {
            m = JSON.parse(line);
        } catch (e) {
            console.warn("drives: bad line", e);
            return;
        }
        switch (m.type) {
        case "hello":
            hello = m;
            connected = true;
            for (const c of queued)
                proc.write(JSON.stringify(c) + "\n");
            queued = [];
            break;
        case "drives":
            drives = m.drives;
            if (wantedDevice !== "")
                pickWanted();
            // Keep the choice while it is there; else the first drive.
            if (!drives.some(d => d.id === driveId) && !busy)
                driveId = drives.length > 0 ? drives[0].id : "";
            break;
        case "images":
            images = m.images;
            if (!images.some(i => i.path === imagePath))
                imagePath = images.length > 0 ? images[0].path : "";
            break;
        case "job":
            // Only ours (a refused second job reports with its own id).
            if (job && m.job.id === job.id) {
                job = Object.assign({}, job, m.job);
                // A kept image: show it from the folder from now on.
                if (job.kind === "add" && job.phase === "done" && job.path)
                    imagePath = job.path;
            }
            break;
        }
    }

    Component.onCompleted: {
        const init = Quickshell.env("VELA_DRIVES_OPEN");
        if (init) {
            try {
                open(JSON.parse(init));
            } catch (e) {
                console.warn("drives: bad VELA_DRIVES_OPEN", e);
            }
        }
    }

    Process {
        id: proc

        command: [Quickshell.env("VELA_BIN") || "vela", "drives", "serve"]
        running: true
        stdinEnabled: true
        stdout: SplitParser {
            onRead: line => root.apply(line)
        }
        stderr: SplitParser {
            onRead: line => console.warn("drives:", line)
        }
        onExited: code => {
            root.connected = false;
            if (root.busy)
                root.job = Object.assign({}, root.job, {
                    phase: "failed",
                    reason: "backend"
                });
            console.warn("drives: backend exited with", code);
            retry.start();
        }
    }

    Timer {
        id: retry

        interval: 1500
        onTriggered: proc.running = true
    }
}
