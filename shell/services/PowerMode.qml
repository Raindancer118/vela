pragma Singleton

import QtQuick
import Quickshell
import Quickshell.Services.UPower
import qs

// Power profile (power-profiles-daemon via UPower): the tile cycles
// power saver → balanced → performance.
Singleton {
    id: root

    readonly property int profile: PowerProfiles.profile
    readonly property bool hasPerformance: PowerProfiles.hasPerformanceProfile

    function next(current: int, performance: bool): int {
        if (current === PowerProfile.PowerSaver)
            return PowerProfile.Balanced;
        if (current === PowerProfile.Balanced && performance)
            return PowerProfile.Performance;
        return PowerProfile.PowerSaver;
    }

    function icon(p: int): string {
        if (p === PowerProfile.PowerSaver)
            return "energy_savings_leaf";
        return p === PowerProfile.Performance ? "speed" : "balance";
    }

    function label(p: int): string {
        if (p === PowerProfile.PowerSaver)
            return I18n.tr("Power saver");
        return p === PowerProfile.Performance ? I18n.tr("Performance") : I18n.tr("Balanced");
    }

    function cycle(): void {
        PowerProfiles.profile = next(profile, hasPerformance);
    }
}
