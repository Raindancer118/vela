pragma Singleton

import QtQuick
import Quickshell
import qs
import Quickshell.Services.UPower

Singleton {
    id: root

    readonly property UPowerDevice device: UPower.displayDevice
    // False on desktops (or while UPower is not running).
    readonly property bool available: device.ready && device.isPresent && device.type === UPowerDeviceType.Battery
    readonly property int percent: Math.round(device.percentage * 100)
    readonly property bool charging: device.state === UPowerDeviceState.Charging || device.state === UPowerDeviceState.PendingCharge
    readonly property bool full: device.state === UPowerDeviceState.FullyCharged
    readonly property bool pluggedIn: !UPower.onBattery

    readonly property string icon: {
        if (charging || (pluggedIn && !full))
            return "battery_charging_full";
        if (full || percent >= 95)
            return "battery_full";
        if (percent <= 10)
            return "battery_alert";
        return "battery_" + Math.max(0, Math.min(6, Math.floor(percent / 100 * 7))) + "_bar";
    }

    readonly property string stateText: {
        if (full)
            return I18n.tr("Fully charged");
        if (charging)
            return device.timeToFull > 0 ? I18n.tr("Charging · %1 until full", formatDuration(device.timeToFull)) : I18n.tr("Charging");
        if (pluggedIn)
            return I18n.tr("Plugged in");
        return device.timeToEmpty > 0 ? I18n.tr("%1 left", formatDuration(device.timeToEmpty)) : I18n.tr("On battery");
    }

    function formatDuration(seconds: real): string {
        return I18n.duration(seconds);
    }
}
