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

// The daemon's recording for the long graphs (1 h at its interval, 24 h of
// minute averages); replaced as a whole by each "history" reply.
var long = { series: {}, coarse: {}, interval: 3000, coarseInterval: 60000 };

function setLong(b) {
    // A reply carries either the last hour (series) or a longer range (coarse).
    long = {
        series: b.interval ? (b.series || {}) : long.series,
        coarse: b.coarse_interval ? (b.coarse || {}) : long.coarse,
        interval: b.interval || long.interval,
        coarseInterval: b.coarse_interval || long.coarseInterval
    };
}

// Points of `key` covering `secs` and the milliseconds between them; more
// than ~1500 points are averaged in buckets (gaps stay gaps).
function longGet(key, secs) {
    const fine = secs <= 3600;
    let step = fine ? long.interval : long.coarseInterval;
    let n = Math.ceil(secs * 1000 / step);
    let a = ((fine ? long.series : long.coarse)[key] || []).slice(-n);
    const bucket = Math.max(1, Math.ceil(n / 1500));
    if (bucket > 1) {
        const out = [];
        for (let i = a.length % bucket; i < a.length; i += bucket) {
            const part = a.slice(i, i + bucket).filter(v => v !== null);
            out.push(part.length > 0 ? part.reduce((s, v) => s + v, 0) / part.length : null);
        }
        a = out;
        step *= bucket;
        n = Math.ceil(n / bucket);
    }
    return { values: a, step: step, points: n };
}

// Forgets per-app series of apps that are gone.
function prune(prefixes, alive) {
    for (const k in data)
        for (const p of prefixes)
            if (k.startsWith(p) && !alive[k.slice(p.length)])
                delete data[k];
}
