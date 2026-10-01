// Mini player logic: which MPRIS player is shown, time labels.
//   qs -p test-media.qml   (scripts/shell-test.sh); prints PASS or FAIL
import QtQuick
import Quickshell
import Quickshell.Services.Mpris
import qs
import qs.services

ShellRoot {
    Component.onCompleted: {
        const spotify = { identity: "Spotify", desktopEntry: "spotify", dbusName: "org.mpris.MediaPlayer2.spotify", isPlaying: false, trackTitle: "Song" };
        const brave = { identity: "Brave", desktopEntry: "brave-browser", dbusName: "org.mpris.MediaPlayer2.brave.instance1", isPlaying: true, trackTitle: "Video" };
        const idle = { identity: "mpv", desktopEntry: "mpv", dbusName: "org.mpris.MediaPlayer2.mpv", isPlaying: false, trackTitle: "" };
        const name = p => p ? p.identity : null;
        const got = [
            name(Media.pick([brave, spotify], false)),
            name(Media.pick([brave, idle], false)),
            name(Media.pick([idle, brave, spotify], true)),
            name(Media.pick([idle, spotify], true)),
            name(Media.pick([idle], true)),
            name(Media.pick([], true)),
            Media.isSpotify(spotify), Media.isSpotify(brave),
            Media.time(0), Media.time(65.4), Media.time(3725), Media.time(-1),
            Media.nextLoop(MprisLoopState.None), Media.nextLoop(MprisLoopState.Playlist), Media.nextLoop(MprisLoopState.Track),
            Media.loopIcon(MprisLoopState.None), Media.loopIcon(MprisLoopState.Playlist), Media.loopIcon(MprisLoopState.Track)
        ];
        const expected = ["Spotify", null, "Brave", "Spotify", "mpv", null, true, false, "0:00", "1:05", "1:02:05", "0:00",
            MprisLoopState.Playlist, MprisLoopState.Track, MprisLoopState.None, "repeat", "repeat", "repeat_one"];
        const ok = JSON.stringify(got) === JSON.stringify(expected);
        console.log((ok ? "PASS " : "FAIL ") + JSON.stringify(got) + (ok ? "" : "\n  expected " + JSON.stringify(expected)));
        Qt.callLater(Qt.quit);
    }
}
