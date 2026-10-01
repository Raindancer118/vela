import QtQuick
import qs

// Small uppercase heading, like the launcher's "APPLICATIONS" / "FILES".
StyledText {
    color: Theme.colors.textMuted
    font.pixelSize: Theme.font.section
    font.weight: Font.Bold
    font.capitalization: Font.AllUppercase
    font.letterSpacing: Theme.font.section * 0.06
}
