import QtQuick
import qs

// Vertical Flickable around a column of content. Its implicit height is the
// content height, so layouts can shrink it and let it scroll.
Flickable {
    id: root

    default property alias content: column.data
    property alias spacing: column.spacing

    implicitHeight: column.implicitHeight
    contentHeight: column.implicitHeight
    contentWidth: width
    clip: true
    boundsBehavior: Flickable.StopAtBounds

    Column {
        id: column

        width: root.width
        spacing: Theme.spacing.xs
    }

    // Thin scroll indicator.
    Rectangle {
        visible: root.contentHeight > root.height
        anchors.right: parent.right
        y: root.visibleArea.yPosition * root.height
        width: Theme.size.scrollbarWidth
        height: root.visibleArea.heightRatio * root.height
        radius: width / 2
        color: Theme.colors.textDisabled
        opacity: root.moving ? 1 : Theme.opacity.scrollbarIdle
    }
}
