import QtQuick
import Quickshell
import qs
import qs.components
import "Words.js" as W

// The system's logo from the icon theme; without one two letters on its
// colour ("Ar" on Arch blue), and a disc for unknown systems.
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

    implicitWidth: size
    implicitHeight: size
    radius: Math.round(size * 0.3)
    color: logo !== "" ? Theme.colors.chip : known ? Theme.withAlpha(brand, 0.2) : Theme.colors.chip
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

    StyledText {
        visible: root.known && root.logo === ""
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
