.pragma library

// Pure helpers of the share picker (tested by test-sharepicker.qml).

// Monitors scaled into a box of w×h as they are arranged on the desk.
// screens: [{name, x, y, width, height}] in logical pixels. Returns
// [{name, x, y, width, height}] in box pixels, each shrunk by gap/2 per side.
function screenLayout(screens, w, h, gap) {
    if (screens.length === 0 || w <= 0 || h <= 0)
        return [];
    const minX = Math.min(...screens.map(s => s.x));
    const minY = Math.min(...screens.map(s => s.y));
    const maxX = Math.max(...screens.map(s => s.x + s.width));
    const maxY = Math.max(...screens.map(s => s.y + s.height));
    const scale = Math.min(w / (maxX - minX), h / (maxY - minY));
    const offX = (w - (maxX - minX) * scale) / 2;
    const offY = (h - (maxY - minY) * scale) / 2;
    return screens.map(s => ({
                name: s.name,
                x: Math.round(offX + (s.x - minX) * scale + gap / 2),
                y: Math.round(offY + (s.y - minY) * scale + gap / 2),
                width: Math.round(s.width * scale - gap),
                height: Math.round(s.height * scale - gap)
            }));
}

// Columns for n tiles of aspect `ratio` plus `extra` px of text: the fewest
// in [min, max] that fit them all into w×h, else max.
function gridColumns(n, w, h, ratio, extra, min, max) {
    for (let c = min; c < max; c++) {
        const cellH = Math.round(Math.floor(w / c) * ratio) + extra;
        if (Math.ceil(n / c) * cellH <= h)
            return c;
    }
    return max;
}

// Monitor caption: built-in panels have no useful model name ("0x150C").
function screenTitle(name, model, builtIn) {
    if (/^(eDP|LVDS|DSI)/.test(name))
        return builtIn;
    return model || name;
}

// Windows whose title or class contain every word of the query.
function filterWindows(windows, query) {
    const words = query.toLowerCase().split(/\s+/).filter(w => w !== "");
    return windows.filter(win => {
        const hay = (win.title + " " + win.className).toLowerCase();
        return words.every(w => hay.includes(w));
    });
}

// Regular workspaces in order, then special ones (minimized, scratchpads).
function sortWindows(windows) {
    const rank = w => w.workspaceId > 0 ? w.workspaceId : 100000 - w.workspaceId;
    return windows.map((w, i) => ({ w, i })).sort((a, b) => rank(a.w) - rank(b.w) || a.i - b.i).map(e => e.w);
}

// Next index in a grid of `cols` columns; stays put at the edges.
function gridMove(index, count, cols, key) {
    if (count === 0)
        return -1;
    if (index < 0)
        return 0;
    let next = index;
    switch (key) {
    case "left":
        next = index % cols === 0 ? index : index - 1;
        break;
    case "right":
        next = index % cols === cols - 1 ? index : index + 1;
        break;
    case "up":
        next = index - cols;
        break;
    case "down":
        next = index + cols;
        break;
    }
    return next < 0 || next >= count ? index : next;
}

// Screens laid out on the desk: the nearest one in a direction.
function nearestScreen(rects, index, key) {
    const cur = rects[index];
    if (!cur)
        return rects.length > 0 ? 0 : -1;
    const cx = r => r.x + r.width / 2;
    const cy = r => r.y + r.height / 2;
    let best = index;
    let bestDist = Infinity;
    rects.forEach((r, i) => {
        const dx = cx(r) - cx(cur);
        const dy = cy(r) - cy(cur);
        const ok = key === "left" ? dx < 0 : key === "right" ? dx > 0 : key === "up" ? dy < 0 : dy > 0;
        const dist = Math.abs(dx) + Math.abs(dy);
        if (i !== index && ok && dist < bestDist) {
            best = i;
            bestDist = dist;
        }
    });
    return best;
}

function answer(kind, value, token) {
    const a = { kind: kind, token: token };
    if (kind === "screen")
        a.output = value;
    if (kind === "window")
        a.handle = value;
    return JSON.stringify(a);
}
