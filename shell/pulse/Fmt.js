.pragma library

// Number formatting for Pulse. `loc` is a Qt locale (decimal comma in German).

function num(v, digits, loc) {
    return Number(v).toLocaleString(loc, "f", digits);
}

// 1536 → "1.5 KB"; binary units (like the kernel counts), short names.
function bytes(b, loc) {
    b = Math.max(0, Number(b) || 0);
    const units = ["B", "KB", "MB", "GB", "TB"];
    let i = 0;
    while (b >= 1024 && i < units.length - 1) {
        b /= 1024;
        i++;
    }
    const digits = i === 0 ? 0 : b >= 100 ? 0 : b >= 10 ? 1 : 2;
    return num(b, digits, loc) + " " + units[i];
}

function rate(bps, loc) {
    return bytes(bps, loc) + "/s";
}

// Network: bits per second, like ISPs and routers.
function bits(bps, loc) {
    let b = Math.max(0, Number(bps) || 0) * 8;
    const units = ["bit/s", "kbit/s", "Mbit/s", "Gbit/s"];
    let i = 0;
    while (b >= 1000 && i < units.length - 1) {
        b /= 1000;
        i++;
    }
    return num(b, i === 0 ? 0 : b >= 100 ? 0 : 1, loc) + " " + units[i];
}

function percent(v, loc, digits) {
    const d = digits === undefined ? (v > 0 && v < 10 ? 1 : 0) : digits;
    return num(Math.max(0, v), d, loc) + " %";
}

function mhz(v, loc) {
    return v >= 1000 ? num(v / 1000, 2, loc) + " GHz" : num(v, 0, loc) + " MHz";
}

function celsius(v, loc) {
    return num(v, 0, loc) + " °C";
}

function watts(v, loc) {
    return num(v, v < 10 ? 1 : 0, loc) + " W";
}

// Seconds → "2:05:09" style uptime pieces are done by I18n; this is the
// clock-like form used in tables.
function clock(seconds) {
    seconds = Math.max(0, Math.floor(seconds));
    const h = Math.floor(seconds / 3600);
    const m = Math.floor(seconds % 3600 / 60);
    const s = seconds % 60;
    const pad = n => (n < 10 ? "0" : "") + n;
    return h > 0 ? h + ":" + pad(m) + ":" + pad(s) : m + ":" + pad(s);
}

// Upper bound for a graph: a "nice" number ≥ the largest value.
function niceMax(values, floor) {
    let m = floor || 0;
    for (const v of values)
        if (v > m)
            m = v;
    if (m <= 0)
        return 1;
    const p = Math.pow(10, Math.floor(Math.log10(m)));
    for (const f of [1, 1.5, 2, 2.5, 3, 4, 5, 6, 8, 10])
        if (f * p >= m)
            return f * p;
    return 10 * p;
}

// Heat (0–1) for the process table cells.
function heat(v, full) {
    return Math.max(0, Math.min(1, v / full));
}
