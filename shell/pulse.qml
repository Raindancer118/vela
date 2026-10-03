//@ pragma IconTheme Papirus-Dark

// Entry point of Pulse, the task manager: `vela pulse` runs
// `qs -p pulse.qml` (its own instance, next to the control center).
import QtQuick
import Quickshell
import Quickshell.Io
import qs.pulse

ShellRoot {
    PulseWindow {
        id: win
    }

    // vela pulse (again): bring the window to the front; `page` opens a page.
    IpcHandler {
        target: "pulse"

        function show(): void {
            win.raise();
        }

        function page(name: string): void {
            PulseUi.show(name);
            win.raise();
        }

        function quit(): void {
            Qt.quit();
        }
    }
}
