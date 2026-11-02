.pragma library

// Formatting for the Drives window. Sizes in decimal units: what is printed
// on the stick ("16 GB"), not what a kernel counts (14.5 GiB).

function size(b, loc) {
    if (!b)
        return "0 B";
    const units = [["TB", 1e12], ["GB", 1e9], ["MB", 1e6], ["KB", 1e3]];
    for (const [u, f] of units)
        if (b >= f) {
            const v = b / f;
            return Number(v).toLocaleString(loc, "f", v >= 100 ? 0 : 1) + " " + u;
        }
    return b + " B";
}

// The size a stick is sold as: 15.58 GB of a "16 GB" stick.
function nominal(b, loc) {
    const sizes = [1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048];
    const gb = b / 1e9;
    const n = sizes.find(s => gb <= s && gb >= s * 0.88);
    return n ? (n >= 1024 ? (n / 1024) + " TB" : n + " GB") : size(b, loc);
}

function rate(bps, loc) {
    return size(bps, loc) + "/s";
}

// Seconds left as "2 min" / "40 s" (tr: I18n.tr).
function left(seconds, tr) {
    if (!isFinite(seconds) || seconds <= 0)
        return "";
    if (seconds < 60)
        return tr("%1 s left", Math.max(1, Math.round(seconds)));
    if (seconds < 3600)
        return tr("%1 min left", Math.round(seconds / 60));
    return tr("%1 h %2 min left", Math.floor(seconds / 3600), Math.round(seconds % 3600 / 60));
}

// Monogram colours per system family (dark, light).
const families = {
    arch: ["#1793d1", "#0f6e9e"],
    cachyos: ["#00ccb4", "#00897b"],
    endeavouros: ["#a77ef0", "#7e57c2"],
    manjaro: ["#35bf5c", "#2e7d32"],
    ubuntu: ["#e95420", "#c43e0f"],
    kubuntu: ["#3f8fd2", "#1e6aa8"],
    debian: ["#d70a53", "#a80740"],
    fedora: ["#51a2da", "#294172"],
    mint: ["#87cf3e", "#5a9a1a"],
    pop: ["#48b9c7", "#2b8a96"],
    nixos: ["#7ebae4", "#5277c3"],
    suse: ["#73ba25", "#4d8a12"],
    raspberry: ["#e0306b", "#c51a4a"],
    proxmox: ["#e57000", "#b85a00"],
    truenas: ["#0095d5", "#006da0"],
    homeassistant: ["#41bdf5", "#038fc7"],
    windows: ["#3a96dd", "#0067b8"],
    kali: ["#557c94", "#367bf0"],
    tails: ["#8e5fd6", "#56347c"],
    alpine: ["#2e8bb9", "#0d597f"],
    freebsd: ["#d4232f", "#ab1520"],
    steamos: ["#5c9fd6", "#1a5c99"],
    bazzite: ["#9b6ce0", "#6a3fb5"],
    garuda: ["#e45d97", "#b83a72"],
    gparted: ["#e8b21c", "#a87a00"],
    clonezilla: ["#e8b21c", "#a87a00"],
    memtest: ["#6aa84f", "#38761d"],
    rocky: ["#10b981", "#047857"],
    alma: ["#ffcb4c", "#b8860b"],
    centos: ["#a14f8c", "#6f2f60"],
    rhel: ["#ee0000", "#b00000"],
    elementary: ["#64baff", "#1e6ab0"],
    zorin: ["#15a6f0", "#0b72a8"],
    void: ["#6abf69", "#2e7d32"],
    gentoo: ["#a59fd8", "#54487a"],
    opnsense: ["#e8762b", "#b8571a"],
    pfsense: ["#5b9bd5", "#212121"],
    unraid: ["#f15a2c", "#b8401a"],
    libreelec: ["#61c1e7", "#17809b"],
    retropie: ["#cc2e2e", "#991f1f"]
};

// Bundled monochrome logos (Simple Icons, assets/logos/<family>.svg).
const bundled = ["arch", "cachyos", "endeavouros", "manjaro", "ubuntu", "kubuntu", "debian", "fedora", "mint", "pop", "elementary", "zorin", "kali",
    "tails", "raspberry", "nixos", "suse", "alpine", "void", "gentoo", "rhel", "centos", "rocky", "alma", "steamos", "freebsd", "proxmox", "truenas",
    "homeassistant", "libreelec", "retropie", "opnsense", "pfsense", "unraid", "garuda"];

function bundledLogo(family) {
    return bundled.includes(family) ? family : "";
}

function familyColor(family, light) {
    const c = families[family];
    return c ? c[light ? 1 : 0] : "";
}

// Two letters for the tile: "Arch Linux" → "Ar", "Pop!_OS" → "Po".
function monogram(system) {
    const letters = (system || "?").replace(/[^A-Za-z0-9]/g, "");
    return letters.length === 0 ? "?" : letters.charAt(0).toUpperCase() + (letters.length > 1 ? letters.charAt(1).toLowerCase() : "");
}

// Icon names that carry a system's logo (Papirus' distributor-logo-*,
// apps' own icons); the first the icon theme has wins.
const logos = {
    arch: ["distributor-logo-archlinux", "archlinux-logo"],
    cachyos: ["org.cachyos.hello", "cachyos"],
    endeavouros: ["distributor-logo-endeavouros"],
    manjaro: ["distributor-logo-manjaro"],
    ubuntu: ["distributor-logo-ubuntu"],
    kubuntu: ["distributor-logo-kubuntu", "distributor-logo-ubuntu"],
    debian: ["distributor-logo-debian", "debian-logo"],
    fedora: ["distributor-logo-fedora"],
    mint: ["distributor-logo-linux-mint"],
    pop: ["distributor-logo-pop-os"],
    elementary: ["distributor-logo-elementary", "io.elementary.appcenter"],
    zorin: ["distributor-logo-zorin"],
    kali: ["distributor-logo-kali-linux"],
    tails: ["distributor-logo-tails", "tails"],
    raspberry: ["distributor-logo-raspbian", "rpi-imager"],
    nixos: ["distributor-logo-nixos"],
    suse: ["distributor-logo-opensuse"],
    alpine: ["distributor-logo-alpine"],
    void: ["distributor-logo-void"],
    gentoo: ["distributor-logo-gentoo", "icon_gentoo_small"],
    rhel: ["distributor-logo-rhel", "redhat"],
    centos: ["distributor-logo-centos"],
    rocky: ["distributor-logo-rocky"],
    alma: ["distributor-logo-almalinux"],
    steamos: ["distributor-logo-steamos", "steam"],
    freebsd: ["distributor-logo-freebsd"],
    windows: ["distributor-logo-windows"],
    gparted: ["gparted"],
    clonezilla: ["clonezilla"],
    retropie: ["retropie"],
    libreelec: ["kodi"],
    ventoy: ["ventoy"],
    proxmox: ["proxmox", "distributor-logo-proxmox"],
    truenas: ["truenas"],
    homeassistant: ["home-assistant", "homeassistant"]
};

function logoNames(family) {
    return logos[family] ?? [];
}
