pragma Singleton

import QtQuick
import Quickshell
import qs
import "Words.js" as W

// The words of the Drives window: names of drives, what is on them, what a
// job is doing and why it failed.
Singleton {
    function icon(d: var): string {
        if (!d)
            return "usb_drive";
        return d.kind === "sd" ? "sd_card" : d.kind === "loop" ? "disc" : "usb_drive";
    }

    function name(d: var): string {
        if (!d)
            return "";
        if (d.name)
            return d.name;
        return d.kind === "sd" ? I18n.tr("SD card") : d.kind === "loop" ? I18n.tr("Test drive (%1)", d.device) : I18n.tr("USB stick");
    }

    function fsName(fs: string): string {
        return {
            vfat: "FAT32",
            exfat: "exFAT",
            ntfs: "NTFS",
            ext4: "ext4",
            btrfs: "Btrfs",
            iso9660: I18n.tr("ISO image"),
            crypto_LUKS: I18n.tr("encrypted")
        }[fs] ?? fs;
    }

    // "Ventoy · 3 images", "STICK · FAT32", "Arch Linux ISO", "Empty".
    function contents(d: var): string {
        if (!d)
            return "";
        if (d.ventoy) {
            const n = d.ventoy.files?.length ?? 0;
            return d.ventoy.mount ? I18n.tr("Ventoy · %1 images", n) : "Ventoy";
        }
        const vols = d.volumes.filter(v => v.fs !== "" && v.label !== "VTOYEFI");
        if (vols.length === 0)
            return I18n.tr("Empty");
        const v = vols[0];
        const parts = [v.label, fsName(v.fs)].filter(s => s);
        return parts.join(" · ") + (vols.length > 1 ? " +" + (vols.length - 1) : "");
    }

    function mounted(d: var): bool {
        return !!d && d.volumes.some(v => v.mount_points.length > 0);
    }

    function phase(j: var): string {
        if (!j)
            return "";
        switch (j.phase) {
        case "start":
            return I18n.tr("Starting…");
        case "unmount":
            return I18n.tr("Unmounting…");
        case "auth":
            return I18n.tr("Waiting for your password…");
        case "ventoy":
            return I18n.tr("Installing Ventoy…");
        case "write":
            return I18n.tr("Writing the image");
        case "verify":
            return I18n.tr("Checking what was written");
        case "finish":
            return I18n.tr("Finishing…");
        case "erase":
            return I18n.tr("Erasing everything");
        case "format":
            return I18n.tr("Formatting…");
        case "mount":
            return I18n.tr("Mounting…");
        case "copy":
            return j.kind === "add" ? I18n.tr("Copying into the image folder") : I18n.tr("Copying onto the stick");
        case "done":
            return {
                write: j.ventoy ? I18n.tr("The image is on the stick") : I18n.tr("The stick is ready to boot"),
                format: j.ventoy ? I18n.tr("Ventoy is on the stick") : I18n.tr("The stick is formatted"),
                eject: I18n.tr("You can unplug the stick now"),
                add: I18n.tr("The image is in the folder"),
                remove: I18n.tr("Deleted from the stick")
            }[j.kind] ?? I18n.tr("Done");
        case "cancelled":
            return I18n.tr("Cancelled");
        case "failed":
            return I18n.tr("That didn't work");
        }
        return "";
    }

    // What to tell after `phase` for the job.
    function detail(j: var): string {
        if (!j)
            return "";
        if (j.phase === "auth")
            return I18n.tr("Writing to a drive directly needs your permission once.");
        if (j.phase === "ventoy")
            return I18n.tr("Ventoy needs administrator rights; the system asks for your password.");
        if (j.phase === "erase")
            return I18n.tr("Every byte is overwritten with zeros. That takes a while.");
        if (j.phase === "cancelled")
            return j.kind === "write" ? I18n.tr("The stick is only half written now; format it or write again.") : "";
        if (j.phase === "done") {
            if (j.kind === "write" && !j.ventoy)
                return j.verified ? I18n.tr("Written and read back: every byte matches.") : I18n.tr("Written (not read back).");
            if (j.kind === "write")
                return I18n.tr("Boot from the stick and pick it in Ventoy's menu.");
            if (j.kind === "format" && j.ventoy)
                return I18n.tr("Copy ISO files onto it; Ventoy lists them when you boot from the stick.");
            if (j.kind === "format")
                return j.mount ? I18n.tr("Mounted at %1.", j.mount) : "";
            return "";
        }
        if (j.phase !== "failed")
            return "";
        switch (j.reason) {
        case "auth":
            return I18n.tr("The password was not entered, so nothing was changed.");
        case "busy":
            return I18n.tr("A program still uses the stick. Close it (or its folder) and try again.");
        case "gone":
            return I18n.tr("The stick is gone. Plug it in again.");
        case "space":
            return I18n.tr("The image is bigger than the stick.");
        case "verify":
            return I18n.tr("The stick reads back different data. It may be worn out; try another one.");
        case "unpack":
            return I18n.tr("The image file is damaged (unpacking failed). Download it again.");
        case "readonly":
            return I18n.tr("The stick is write-protected. Check the little switch on its side.");
        case "running":
            return I18n.tr("Something else is still running.");
        case "backend":
            return I18n.tr("The background helper stopped. Try again.");
        }
        return j.error ?? "";
    }

    function progressLine(j: var): string {
        if (!j || !j.total)
            return "";
        const loc = Qt.locale();
        const parts = [I18n.tr("%1 of %2", W.size(j.done, loc), W.size(j.total, loc))];
        if (j.rate > 0) {
            parts.push(W.rate(j.rate, loc));
            const left = W.left((j.total - j.done) / j.rate, I18n.tr);
            if (left)
                parts.push(left);
        }
        return parts.join("  ·  ");
    }
}
