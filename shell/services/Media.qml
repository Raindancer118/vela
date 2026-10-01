pragma Singleton

import QtQuick
import Quickshell
import Quickshell.Services.Mpris
import qs

// The MPRIS player the panel's mini player shows: Spotify, or with
// "any player" the one playing (else the first with a track).
Singleton {
    id: root

    readonly property var player: Config.mediaPlayer ? pick(Mpris.players.values, Config.mediaPlayerAny) : null
    readonly property bool available: player !== null && player.trackTitle !== ""

    function isSpotify(p: var): bool {
        return [p.identity, p.desktopEntry, p.dbusName].some(s => (s || "").toLowerCase().includes("spotify"));
    }

    function pick(players: var, any: bool): var {
        const list = Array.from(players).filter(p => any || isSpotify(p));
        return list.find(p => p.isPlaying) ?? list.find(p => p.trackTitle) ?? list[0] ?? null;
    }

    // Repeat button: off → playlist → track → off.
    function nextLoop(state: int): int {
        if (state === MprisLoopState.None)
            return MprisLoopState.Playlist;
        return state === MprisLoopState.Playlist ? MprisLoopState.Track : MprisLoopState.None;
    }

    function loopIcon(state: int): string {
        return state === MprisLoopState.Track ? "repeat_one" : "repeat";
    }

    function time(seconds: real): string {
        const s = Math.max(0, Math.floor(seconds));
        const two = n => String(n).padStart(2, "0");
        const h = Math.floor(s / 3600), m = Math.floor(s / 60) % 60;
        return h > 0 ? h + ":" + two(m) + ":" + two(s % 60) : m + ":" + two(s % 60);
    }

    // MPRIS doesn't signal the position while playing; ask once a second
    // while someone can see it.
    Timer {
        running: root.player !== null && root.player.isPlaying && ShellState.panelOpen && Config.mediaPlayerProgress
        interval: 1000
        repeat: true
        onTriggered: root.player.positionChanged()
    }
}
