pragma Singleton

import QtQuick
import Quickshell

// Every visual constant of the shell. Components must not hardcode colors,
// radii, spacings, fonts or durations; add a token here instead.
Singleton {
    id: root

    // Colours, radius, text size, opacity and speed follow the vela
    // settings (VelaConfig); surfaces are the theme's tint over the panel.
    readonly property var vc: VelaConfig.colors
    readonly property var va: VelaConfig.appearance

    function withAlpha(c: color, a: real): color {
        return Qt.rgba(c.r, c.g, c.b, Math.max(0, Math.min(1, a)));
    }

    readonly property QtObject colors: QtObject {
        readonly property color background: root.vc.background
        readonly property color tint: root.vc.tint
        readonly property real surfaceBase: root.va.surfaceOpacity
        readonly property color surface: root.withAlpha(tint, surfaceBase)
        readonly property color surfaceHigh: root.withAlpha(tint, surfaceBase + 0.05)
        readonly property color surfaceHighest: root.withAlpha(tint, surfaceBase + 0.1)
        readonly property color outline: root.withAlpha(tint, 0.1)
        // Panel and detail window background (translucent; Hyprland blurs it).
        readonly property color panel: root.withAlpha(background, root.surfaceOpacity)

        readonly property color primary: root.vc.primary
        readonly property color textOnPrimary: root.vc.textOnPrimary
        readonly property color primaryMuted: root.vc.primaryMuted

        readonly property color text: root.vc.text
        readonly property color textMuted: root.vc.textMuted
        readonly property color textDisabled: root.vc.textDisabled

        readonly property color error: root.vc.error
        readonly property color textOnError: root.vc.textOnError
        readonly property color errorSurface: root.vc.errorSurface

        // State layers drawn over a surface while hovered / pressed.
        readonly property color hover: root.withAlpha(tint, 0.07)
        readonly property color pressed: root.withAlpha(tint, 0.12)
        readonly property color hoverOnPrimary: Qt.rgba(0, 0, 0, 0.08)
        readonly property color pressedOnPrimary: Qt.rgba(0, 0, 0, 0.14)

        // Icon circle inside a toggle tile.
        readonly property color tileIcon: surfaceHighest
        readonly property color tileIconActive: Qt.rgba(0, 0, 0, 0.1)
        readonly property color textOnPrimaryMuted: root.withAlpha(textOnPrimary, 0.7)

        // Claude usage bars: plenty left, getting close, nearly used up.
        readonly property color usageLow: root.va.light ? "#2e7d32" : "#9ece6a"
        readonly property color usageMid: root.va.light ? "#b26a00" : "#e0af68"
        readonly property color usageHigh: error

        // Workspace OSD dots.
        readonly property color dotEmpty: root.withAlpha(text, 0.22)
        readonly property color dotOccupied: root.withAlpha(text, 0.6)
    }

    readonly property QtObject radius: QtObject {
        readonly property int panel: root.va.radius
        readonly property int card: Math.max(0, root.va.radius - 4)
        readonly property int small: Math.round(root.va.radius * 0.55)
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

        readonly property real scale: root.va.fontScale
        readonly property int small: Math.round(12 * scale)
        readonly property int body: Math.round(14 * scale)
        readonly property int title: Math.round(16 * scale)
        readonly property int large: Math.round(20 * scale)
        readonly property int clock: Math.round(64 * scale)
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
        readonly property int panelWidth: VelaConfig.panel.width
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
        readonly property int notificationIconCompact: 24
        // Share of a compact row the title may take before the text.
        readonly property real notificationCompactTitleShare: 0.45
        readonly property int notificationBodyLines: 3
        readonly property int notificationBodyLinesExpanded: 40
        // Share of the row the "App · 5 min ago" label may take.
        readonly property real notificationMetaMaxShare: 0.45
        // App icon inset and fallback glyph size, relative to the circle.
        readonly property real notificationIconInset: 0.15
        readonly property real notificationGlyphScale: 0.55
        readonly property int dragDismissThreshold: 120

        readonly property int usageBarHeight: 6
        readonly property int usageLabelWidth: 28
        readonly property int usagePercentWidth: 40
        readonly property int usageResetWidth: 76

        readonly property int osdTopMargin: 14
        readonly property int osdPadding: 10
        readonly property int osdDot: 8
        readonly property int osdDotActive: 26
        readonly property int osdDotSpacing: 7
    }

    // Background opacity of the panel, popups and detail card.
    readonly property real surfaceOpacity: va.opacity
}
