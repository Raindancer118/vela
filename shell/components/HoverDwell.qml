import QtQuick
import qs

// `dwelled` turns true once `hovered` has stayed true for `delay` ms and
// false as soon as it ends; passing over quickly doesn't count.
QtObject {
    id: root

    property bool hovered: false
    property int delay: Config.hoverExpandDelay
    property bool dwelled: false

    onHoveredChanged: {
        if (hovered) {
            timer.restart();
        } else {
            timer.stop();
            dwelled = false;
        }
    }

    property Timer timer: Timer {
        interval: root.delay
        onTriggered: root.dwelled = root.hovered
    }
}
