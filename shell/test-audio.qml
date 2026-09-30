// Audio icons: inputs are always microphones, outputs/apps speakers.
//   qs -p test-audio.qml   (scripts/shell-test.sh); exits 0 on PASS
import QtQuick
import Quickshell
import Quickshell.Services.Pipewire
import qs.services

ShellRoot {
    Component.onCompleted: {
        const node = (type, volume, muted, name) => ({
                type,
                name: name ?? "alsa_x",
                audio: { volume, muted }
            });
        const got = [
            Audio.levelIcon(node(PwNodeType.AudioSource, 0.8, false)),
            Audio.levelIcon(node(PwNodeType.AudioSource, 0.0, false)),
            Audio.levelIcon(node(PwNodeType.AudioSource, 0.8, true)),
            Audio.levelIcon(node(PwNodeType.AudioSink, 0.8, false)),
            Audio.levelIcon(node(PwNodeType.AudioSink, 0.8, true)),
            Audio.levelIcon(node(PwNodeType.AudioOutStream, 0.3, false)),
            Audio.deviceIcon(node(PwNodeType.AudioSource, 1, false, "bluez_input.headset")),
            Audio.deviceIcon(node(PwNodeType.AudioSink, 1, false, "bluez_output.headset")),
            Audio.deviceIcon(node(PwNodeType.AudioSink, 1, false, "alsa_output.hdmi-stereo"))
        ];
        const expected = ["mic", "mic", "mic_off", "volume_up", "volume_off", "volume_down", "mic", "headphones", "tv"];
        const ok = JSON.stringify(got) === JSON.stringify(expected);
        console.log((ok ? "PASS " : "FAIL ") + JSON.stringify(got) + (ok ? "" : "\n  expected " + JSON.stringify(expected)));
        Qt.exit(ok ? 0 : 1);
    }
}
