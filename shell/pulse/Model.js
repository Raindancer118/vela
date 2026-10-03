.pragma library

// Keeps a ListModel of `{ key }` rows in the order of `keys` with as few
// insert/remove/move calls as possible, so ListView transitions animate
// what really changed (new rows, gone rows, rows that moved when sorting).
function sync(model, keys) {
    const want = {};
    for (let i = 0; i < keys.length; i++)
        want[keys[i]] = i;
    for (let i = model.count - 1; i >= 0; i--)
        if (want[model.get(i).key] === undefined)
            model.remove(i);
    for (let i = 0; i < keys.length; i++) {
        const k = keys[i];
        if (i < model.count && model.get(i).key === k)
            continue;
        let j = -1;
        for (let n = i + 1; n < model.count; n++)
            if (model.get(n).key === k) {
                j = n;
                break;
            }
        if (j >= 0)
            model.move(j, i, 1);
        else
            model.insert(i, { key: k });
    }
    while (model.count > keys.length)
        model.remove(model.count - 1);
}

// Case-insensitive match of every word of `query` in any of `fields`.
function matches(query, fields) {
    const q = query.trim().toLowerCase();
    if (q === "")
        return true;
    const hay = fields.filter(f => f !== undefined && f !== null).join(" ").toLowerCase();
    return q.split(/\s+/).every(w => hay.indexOf(w) >= 0);
}

// Fuzzy score for "Run new task" suggestions: prefix > word start > substring.
function score(query, text) {
    const q = query.trim().toLowerCase();
    const t = (text || "").toLowerCase();
    if (q === "")
        return 1;
    if (t.startsWith(q))
        return 3;
    if (t.split(/[\s\-_.]+/).some(w => w.startsWith(q)))
        return 2;
    return t.indexOf(q) >= 0 ? 1 : 0;
}
