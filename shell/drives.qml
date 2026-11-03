//@ pragma IconTheme Papirus-Dark

// Entry point of Drives: `vela drives` runs `qs -p drives.qml` (its own
// instance); `VELA_DRIVES_OPEN` carries what to show first.
import QtQuick
import Quickshell
import Quickshell.Io
import qs.drives

ShellRoot {
    DrivesWindow {
        id: win
    }

    IpcHandler {
        target: "drives"

        // `vela drives` again: show the window with what it asks for.
        function open(request: string): bool {
            if (!win.visible)
                return false;
            try {
                Drives.open(JSON.parse(request));
            } catch (e) {
                console.warn("drives: bad request", e);
            }
            win.raise();
            return true;
        }

        function present(): bool {
            if (!win.visible)
                return false;
            win.raise();
            return true;
        }

        function quit(): void {
            Qt.quit();
        }
    }
}
