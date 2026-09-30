// SlidingHighlight follows its target: placed without animation at first,
// then glides to a new target and ends exactly on it.
//   qs -p test-highlight.qml   (scripts/shell-test.sh); prints PASS or FAIL
import QtQuick
import Quickshell
import qs.components

ShellRoot {
    Item {
        id: host

        width: 200
        height: 300

        SlidingHighlight {
            id: hl

            target: col.children[host.index] ?? null
        }

        Column {
            id: col

            width: parent.width
            spacing: 4

            Repeater {
                model: 3

                Item {
                    width: 200
                    height: 40 + index * 10
                }
            }
        }

        property int index: 0
    }

    Timer {
        id: step

        property var got: []

        interval: 700
        running: true
        onTriggered: {
            got.push([hl.y, hl.height]);
            if (host.index === 0) {
                host.index = 2;
                // Mid-glide the highlight is between the rows.
                Qt.callLater(() => got.push(hl.y < 98 ? "gliding" : "jumped"));
                restart();
                return;
            }
            const expected = [[0, 40], "gliding", [98, 60]];
            const ok = JSON.stringify(got) === JSON.stringify(expected);
            console.log((ok ? "PASS " : "FAIL ") + JSON.stringify(got) + (ok ? "" : "\n  expected " + JSON.stringify(expected)));
            Qt.quit();
        }
    }
}
