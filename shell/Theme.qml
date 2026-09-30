pragma Singleton

import QtQuick
import Quickshell

// Every visual constant of the shell. Components must not hardcode colors,
// radii, spacings, fonts or durations; add a token here instead.
Singleton {
    id: root

    readonly property QtObject colors: QtObject {
        readonly property color background: "#141218"
        readonly property color surface: "#1d1b20"
        readonly property color surfaceHigh: "#26232b"
        readonly property color surfaceHighest: "#322f37"
        readonly property color outline: "#2e2b33"
        // Panel and detail window background (slightly translucent for blur).
        readonly property color panel: Qt.rgba(background.r, background.g, background.b, root.surfaceOpacity)

        readonly property color primary: "#d0cce0"
        readonly property color textOnPrimary: "#1d1b26"
        readonly property color primaryMuted: "#4a4658"

        readonly property color text: "#e6e1e5"
        readonly property color textMuted: "#9a95a0"
        readonly property color textDisabled: "#5e5a64"

        readonly property color error: "#f2b8b5"
        readonly property color textOnError: "#601410"
        readonly property color errorSurface: "#3a1d1f"

        // State layers drawn over a surface while hovered / pressed.
        readonly property color hover: Qt.rgba(1, 1, 1, 0.07)
        readonly property color pressed: Qt.rgba(1, 1, 1, 0.12)
        readonly property color hoverOnPrimary: Qt.rgba(0, 0, 0, 0.08)
        readonly property color pressedOnPrimary: Qt.rgba(0, 0, 0, 0.14)

        // Icon circle inside a toggle tile.
        readonly property color tileIcon: surfaceHighest
        readonly property color tileIconActive: Qt.rgba(0, 0, 0, 0.1)
        readonly property color textOnPrimaryMuted: Qt.rgba(0.11, 0.1, 0.15, 0.7)

        // Workspace OSD dots.
        readonly property color dotEmpty: Qt.rgba(0.9, 0.88, 0.9, 0.22)
        readonly property color dotOccupied: Qt.rgba(0.9, 0.88, 0.9, 0.6)
    }

    readonly property QtObject radius: QtObject {
        readonly property int panel: 24
        readonly property int card: 20
        readonly property int small: 12
    }

    readonly property QtObject spacing: QtObject {
        readonly property int xs: 4
        readonly property int sm: 8
        readonly property int md: 12
        readonly property int lg: 16
        readonly property int xl: 24
    }

    readonly property QtObject font: QtObject {
        readonly property string family: "Rubik"
        readonly property string iconFamily: "Material Symbols Rounded"

        readonly property int small: 12
        readonly property int body: 14
        readonly property int title: 16
        readonly property int large: 20
        readonly property int clock: 64
        readonly property real labelLetterSpacing: 0.4

        readonly property int weightLight: Font.Light
        readonly property int weightNormal: Font.Normal
        readonly property int weightMedium: Font.Medium
        readonly property int weightSemiBold: Font.DemiBold
    }

    readonly property QtObject icon: QtObject {
        readonly property int small: 18
        readonly property int normal: 22
        readonly property int large: 28
        readonly property int huge: 48

        // Material Symbols variable-font axes.
        readonly property int weight: 400
        readonly property int opticalSizeMin: 20
        readonly property int opticalSizeMax: 48
    }

    readonly property QtObject anim: QtObject {
        readonly property int fast: Math.round(150 * Config.animationScale)
        readonly property int normal: Math.round(250 * Config.animationScale)
        readonly property int slow: Math.round(350 * Config.animationScale)
        // Delay between items leaving one after another ("Clear all").
        readonly property int stagger: Math.round(45 * Config.animationScale)
        // At most this many items get their own stagger step.
        readonly property int staggerMaxSteps: 8

        // Easing.OutBack overshoot for small springy state changes.
        readonly property real springOvershoot: 1.2
        // Size an icon shrinks to before it is replaced.
        readonly property real swapScale: 0.6
        // How far status texts move when they change (px).
        readonly property int swapShift: 4

        // Material 3 easing curves (Easing.BezierSpline control points).
        readonly property var standard: [0.2, 0, 0, 1, 1, 1]
        readonly property var emphasizedDecel: [0.05, 0.7, 0.1, 1, 1, 1]

        // How far popups and the detail card slide while fading in (px).
        readonly property int slideDistance: 48
        // Shorter slide for items appearing inside the panel.
        readonly property int slideShort: 16
        // Start scale of the workspace OSD pill.
        readonly property real popInScale: 0.92
        // The panel fades in this many times faster than it slides.
        readonly property real fadeLead: 1.5
    }

    readonly property QtObject opacity: QtObject {
        readonly property real disabled: 0.4
        readonly property real scrollbarIdle: 0.5
    }

    readonly property QtObject size: QtObject {
        // Distance of floating surfaces from the screen edge
        // (matches Hyprland's gaps_out).
        readonly property int screenMargin: 10
        readonly property int border: 1
        // Fallback width of controls that are normally sized by a layout.
        readonly property int controlWidth: 220
        readonly property int scrollbarWidth: 3
        readonly property int panelWidth: 420
        readonly property int panelPadding: 16
        readonly property int detailWidth: 380
        readonly property int tileHeight: 64
        readonly property int tileIcon: 40
        readonly property int sliderHeight: 44
        readonly property int sliderHeightCompact: 34
        readonly property int iconButton: 40
        readonly property int pillButton: 36
        readonly property int listRow: 56
        readonly property int switchWidth: 52
        readonly property int switchHeight: 32
        readonly property int switchKnobOn: 24
        readonly property int switchKnobOff: 16
        readonly property int switchKnobInset: 4
        readonly property int notificationWidth: 400
        readonly property int notificationIcon: 40
        readonly property int notificationBodyLines: 3
        readonly property int notificationBodyLinesExpanded: 40
        // Share of the row the "App · 5 min ago" label may take.
        readonly property real notificationMetaMaxShare: 0.45
        // App icon inset and fallback glyph size, relative to the circle.
        readonly property real notificationIconInset: 0.15
        readonly property real notificationGlyphScale: 0.55
        readonly property int dragDismissThreshold: 120

        readonly property int osdTopMargin: 14
        readonly property int osdPadding: 10
        readonly property int osdDot: 8
        readonly property int osdDotActive: 26
        readonly property int osdDotSpacing: 7
    }

    // Surface opacity of the panel/cards; Hyprland blurs behind them.
    readonly property real surfaceOpacity: 0.97
}
