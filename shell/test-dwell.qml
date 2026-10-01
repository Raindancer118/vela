// HoverDwell: true only after the pointer stayed for `delay`, false at once
// when it leaves; a short pass doesn't count.
//   qs -p test-dwell.qml   (scripts/shell-test.sh); prints PASS or FAIL
import QtQuick
import Quickshell
import qs.components

ShellRoot {
    HoverDwell {
        id: dwell

        delay: 300
    }

    property var got: []

    SequentialAnimation {
        running: true

        ScriptAction {
            script: dwell.hovered = true
        }
        PauseAnimation {
            duration: 100
        }
        ScriptAction {
            script: got.push(dwell.dwelled)
        }
        ScriptAction {
            script: dwell.hovered = false
        }
        PauseAnimation {
            duration: 400
        }
        ScriptAction {
            script: got.push(dwell.dwelled)
        }
        ScriptAction {
            script: dwell.hovered = true
        }
        PauseAnimation {
            duration: 450
        }
        ScriptAction {
            script: got.push(dwell.dwelled)
        }
        ScriptAction {
            script: {
                dwell.hovered = false;
                got.push(dwell.dwelled);
                const expected = [false, false, true, false];
                const ok = JSON.stringify(got) === JSON.stringify(expected);
                console.log((ok ? "PASS " : "FAIL ") + JSON.stringify(got) + (ok ? "" : "\n  expected " + JSON.stringify(expected)));
                Qt.callLater(Qt.quit);
            }
        }
    }
}
