pragma Singleton

import QtQuick
import Quickshell

// Behavior settings (timings, limits, commands). Visuals live in Theme.qml.
Singleton {
    // Workspace OSD
    readonly property int osdTimeout: 1200
    readonly property int osdMinWorkspaces: 5
    readonly property int osdMaxWorkspaces: 10

    // Notifications
    readonly property int popupTimeout: 5000
    // Upper bound for app-requested timeouts (ms).
    readonly property int popupMaxTimeout: 30000
    readonly property int popupMaxVisible: 4
    readonly property bool criticalPopupsStay: true
    // Notifications shown per app group before "Show more".
    readonly property int groupCollapsedCount: 2
    // Refresh rate of "5 min ago" labels while visible (ms).
    readonly property int relativeTimeInterval: 30000

    // Speed of all animations: 1 = normal, 2 = twice as slow, 0 = off.
    readonly property real animationScale: 1

    // Sliders: volume/brightness change per mouse wheel notch.
    readonly property real sliderWheelStep: 0.05

    // Clock and date follow the system locale (LC_TIME), see I18n.qml.
    readonly property var locale: I18n.timeLocale
    readonly property string timeFormat: locale.timeFormat(Locale.ShortFormat)
    readonly property string dateFormat: I18n.withoutYear(locale.dateFormat(Locale.LongFormat))
    readonly property string shortDateFormat: I18n.withoutYear(locale.dateFormat(Locale.ShortFormat))

    // Night light color temperature in Kelvin (hyprsunset).
    readonly property int nightLightTemperature: 4000

    // Session actions
    readonly property var lockCommand: ["hyprlock"]
    readonly property var rebootCommand: ["systemctl", "reboot"]
    readonly property var poweroffCommand: ["systemctl", "poweroff"]
}
