pragma Singleton

import QtQuick
import Quickshell
import qs
import Quickshell.Services.Pipewire

Singleton {
    id: root

    readonly property PwNode sink: Pipewire.defaultAudioSink
    readonly property PwNode source: Pipewire.defaultAudioSource

    readonly property var sinks: Pipewire.nodes.values.filter(n => n.type === PwNodeType.AudioSink)
    readonly property var sources: Pipewire.nodes.values.filter(n => n.type === PwNodeType.AudioSource)
    // Playback streams of applications.
    readonly property var streams: Pipewire.nodes.values.filter(n => n.type === PwNodeType.AudioOutStream)

    readonly property real volume: sink?.audio?.volume ?? 0
    readonly property bool muted: sink?.audio?.muted ?? true
    readonly property bool micMuted: source?.audio?.muted ?? true

    function setVolume(node: PwNode, value: real): void {
        if (!node?.audio)
            return;
        node.audio.muted = false;
        node.audio.volume = Math.max(0, Math.min(1, value));
    }

    function toggleMute(node: PwNode): void {
        if (node?.audio)
            node.audio.muted = !node.audio.muted;
    }

    function setDefault(node: PwNode): void {
        if (node.isSink)
            Pipewire.preferredDefaultAudioSink = node;
        else
            Pipewire.preferredDefaultAudioSource = node;
    }

    function displayName(node: PwNode): string {
        if (!node)
            return I18n.tr("No device");
        return node.nickname || node.description || node.name;
    }

    function deviceIcon(node: PwNode): string {
        if (!node.isSink)
            return "mic";
        const name = node.name.toLowerCase();
        if (name.startsWith("bluez"))
            return "headphones";
        return name.includes("hdmi") ? "tv" : "speaker";
    }

    // Needs the node to be tracked (properties are only bound then).
    function streamName(node: PwNode): string {
        const props = node.properties ?? {};
        return props["application.name"] || node.description || node.name;
    }

    function volumeIcon(volume: real, muted: bool): string {
        if (muted)
            return "volume_off";
        if (volume <= 0.01)
            return "volume_mute";
        return volume < 0.5 ? "volume_down" : "volume_up";
    }

    // Default devices are always bound so the panel shows live values.
    PwObjectTracker {
        objects: [root.sink, root.source].filter(n => n !== null)
    }
}
