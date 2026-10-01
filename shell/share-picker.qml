//@ pragma IconTheme Papirus-Dark

// Entry point of the screen-share picker: `vela share-picker` runs
// `qs -p share-picker.qml` (its own instance, next to the control center).
import Quickshell
import qs.sharepicker

ShellRoot {
    SharePicker {}
}
