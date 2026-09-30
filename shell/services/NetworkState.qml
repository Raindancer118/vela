pragma Singleton

import QtQuick
import Quickshell
import qs
import Quickshell.Io
import Quickshell.Networking

// Named NetworkState to avoid clashing with the Quickshell `Network` type.
Singleton {
    id: root

    readonly property WifiDevice wifi: Networking.devices.values.find(d => d.type === DeviceType.Wifi) ?? null
    readonly property NetworkDevice wired: Networking.devices.values.find(d => d.type === DeviceType.Wired && d.connected) ?? null

    readonly property bool wifiAvailable: wifi !== null
    readonly property bool wifiEnabled: Networking.wifiEnabled
    readonly property bool wifiBlockedByHardware: !Networking.wifiHardwareEnabled
    readonly property WifiNetwork activeNetwork: wifi?.networks.values.find(n => n.connected) ?? null

    // Scan while the Wi-Fi detail view is open; NetworkManager then rescans
    // about every 10 s and `networks` contains every visible access point.
    readonly property bool scanning: ShellState.panelOpen && ShellState.detail === "wifi"
    readonly property bool usable: wifiAvailable && wifiEnabled && !wifiBlockedByHardware

    // Connected first, then saved networks, then by signal (bucketed so rows
    // don't jump around on small fluctuations), then by name.
    readonly property var networks: {
        if (!wifi)
            return [];
        return wifi.networks.values.filter(n => n.name !== "").sort((a, b) => {
            if (a.connected !== b.connected)
                return a.connected ? -1 : 1;
            if (a.known !== b.known)
                return a.known ? -1 : 1;
            const sa = Math.round(a.signalStrength * 4);
            const sb = Math.round(b.signalStrength * 4);
            if (sa !== sb)
                return sb - sa;
            return a.name.localeCompare(b.name);
        });
    }

    readonly property string icon: {
        if (wired)
            return "lan";
        if (!usable)
            return "signal_wifi_off";
        if (!activeNetwork)
            return "signal_wifi_bad";
        return signalIcon(activeNetwork.signalStrength);
    }

    readonly property string status: {
        if (wired)
            return "Ethernet";
        if (!wifiAvailable)
            return I18n.tr("Unavailable");
        if (wifiBlockedByHardware)
            return I18n.tr("Blocked by hardware switch");
        if (!wifiEnabled)
            return I18n.tr("Off");
        if (activeNetwork)
            return activeNetwork.name;
        return wifi.state === ConnectionState.Connecting ? I18n.tr("Connecting…") : I18n.tr("Not connected");
    }

    readonly property bool online: wired !== null || activeNetwork !== null

    // Quickshell rate-limits its own scans to one per 10 s; ask
    // NetworkManager directly for an immediate one.
    function rescan(): void {
        if (wifi && !rescanProc.running)
            rescanProc.running = true;
    }

    // WPA passphrases are 8-63 characters, or 64 hex digits.
    function validPsk(psk: string): bool {
        return (psk.length >= 8 && psk.length <= 63) || /^[0-9a-fA-F]{64}$/.test(psk);
    }

    function toggleWifi(): void {
        Networking.wifiEnabled = !Networking.wifiEnabled;
    }

    function signalIcon(strength: real): string {
        if (strength > 0.8)
            return "signal_wifi_4_bar";
        if (strength > 0.6)
            return "network_wifi_3_bar";
        if (strength > 0.4)
            return "network_wifi_2_bar";
        if (strength > 0.2)
            return "network_wifi_1_bar";
        return "signal_wifi_0_bar";
    }

    function isSecured(network: WifiNetwork): bool {
        return network.security !== WifiSecurityType.Open && network.security !== WifiSecurityType.Owe;
    }

    // Networks we can join with just a password.
    function usesPsk(network: WifiNetwork): bool {
        return [WifiSecurityType.WpaPsk, WifiSecurityType.Wpa2Psk, WifiSecurityType.Sae].includes(network.security);
    }

    function securityName(network: WifiNetwork): string {
        switch (network.security) {
        case WifiSecurityType.Open:
            return I18n.tr("Open");
        case WifiSecurityType.Owe:
            return "Enhanced open";
        case WifiSecurityType.Sae:
            return "WPA3";
        case WifiSecurityType.Wpa2Psk:
            return "WPA2";
        case WifiSecurityType.WpaPsk:
            return "WPA";
        case WifiSecurityType.Wpa2Eap:
        case WifiSecurityType.WpaEap:
        case WifiSecurityType.Wpa3SuiteB192:
            return "Enterprise";
        default:
            return I18n.tr("Secured");
        }
    }

    Process {
        id: rescanProc

        command: ["nmcli", "device", "wifi", "rescan", "ifname", root.wifi?.name ?? ""]
    }

    Binding {
        when: root.wifi !== null
        target: root.wifi
        property: "scannerEnabled"
        value: root.scanning && root.wifiEnabled
    }
}
