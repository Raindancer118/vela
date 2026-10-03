.pragma library

// Graph history, kept in a library so it is one shared JS object (a QML
// `var` property may hand out copies).
var data = {};

function push(key, v, keep) {
    let a = data[key];
    if (!a) {
        a = [];
        data[key] = a;
    }
    a.push(v);
    if (a.length > keep)
        a.splice(0, a.length - keep);
}

function get(key, n) {
    const a = data[key] || [];
    return n > 0 ? a.slice(-n) : a.slice();
}

// Forgets per-app series of apps that are gone.
function prune(prefixes, alive) {
    for (const k in data)
        for (const p of prefixes)
            if (k.startsWith(p) && !alive[k.slice(p.length)])
                delete data[k];
}
