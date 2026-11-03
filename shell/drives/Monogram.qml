import QtQuick
import QtQuick.Effects
import Quickshell
import qs
import qs.components
import "Words.js" as W

// The system's logo: the icon theme's (Papirus, in colour), else the bundled
// one tinted in the system's colour, else two letters on that colour ("Ar"
// on Arch blue); a disc for unknown systems.
Rectangle {
    id: root

    property string system
    property string family
    property int size: 40
    readonly property color brand: W.familyColor(family, VelaConfig.appearance.light ?? false)
    readonly property bool known: family !== ""
    readonly property string logo: {
        for (const name of W.logoNames(family)) {
            const p = Quickshell.iconPath(name, true);
            if (p !== "")
                return p;
        }
        return "";
    }
    readonly property string bundled: logo === "" ? W.bundledLogo(family) : ""
    readonly property bool letters: known && logo === "" && bundled === ""

    implicitWidth: size
    implicitHeight: size
    radius: Math.round(size * 0.3)
    color: known && logo === "" ? Theme.withAlpha(brand, 0.16) : Theme.colors.chip
    border.width: known && logo === "" ? 1 : 0
    border.color: Theme.withAlpha(brand, 0.45)

    Image {
        visible: root.logo !== ""
        anchors.centerIn: parent
        width: Math.round(root.size * 0.72)
        height: width
        sourceSize: Qt.size(width * 2, height * 2)
        source: root.logo
        fillMode: Image.PreserveAspectFit
        smooth: true
        asynchronous: true
    }

    Image {
        id: mono

        anchors.centerIn: parent
        width: Math.round(root.size * 0.55)
        height: width
        sourceSize: Qt.size(width * 2, height * 2)
        source: root.bundled !== "" ? Qt.resolvedUrl("../assets/logos/" + root.bundled + ".svg") : ""
        smooth: true
        visible: false
    }

    // Simple Icons are black: lifted to white, then tinted.
    MultiEffect {
        anchors.fill: mono
        source: mono
        visible: root.bundled !== "" && mono.status === Image.Ready
        brightness: 1
        colorization: 1
        colorizationColor: root.brand
    }

    StyledText {
        visible: root.letters
        anchors.centerIn: parent
        text: W.monogram(root.system)
        color: root.brand
        font.pixelSize: Math.round(root.size * 0.4)
        font.weight: Theme.font.weightSemiBold
    }

    MaterialIcon {
        visible: !root.known
        anchors.centerIn: parent
        icon: "disc"
        size: Math.round(root.size * 0.5)
        color: Theme.colors.textMuted
    }
}
