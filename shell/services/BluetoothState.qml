pragma Singleton

import QtQuick
import Quickshell
import qs
import Quickshell.Bluetooth

// Named BluetoothState to avoid shadowing the Quickshell `Bluetooth` singleton.
Singleton {
    id: root

    readonly property BluetoothAdapter adapter: Bluetooth.defaultAdapter
    readonly property bool available: adapter !== null
    readonly property bool enabled: adapter?.enabled ?? false
    readonly property bool blocked: adapter?.state === BluetoothAdapterState.Blocked

    readonly property var devices: adapter ? adapter.devices.values : []
    readonly property var paired: devices.filter(d => d.paired || d.bonded).sort((a, b) => {
        if (a.connected !== b.connected)
            return a.connected ? -1 : 1;
        return a.name.localeCompare(b.name);
    })
    // Unpaired devices seen while scanning. Devices that don't advertise a
    // name (beacons, random addresses) are hidden.
    readonly property var discovered: devices.filter(d => !d.paired && !d.bonded && d.deviceName !== "").sort((a, b) => a.name.localeCompare(b.name))
    readonly property var connected: paired.filter(d => d.connected)

    // Discover new devices while the Bluetooth detail view is open.
    readonly property bool scanning: ShellState.panelOpen && ShellState.detail === "bluetooth"
    property BluetoothDevice pairingDevice: null
    // Last device whose pairing failed (no agent for PIN/confirmation,
    // rejected, timed out).
    property BluetoothDevice failedDevice: null

    readonly property string status: {
        if (!available)
            return I18n.tr("Unavailable");
        if (blocked)
            return I18n.tr("Blocked");
        if (!enabled)
            return I18n.tr("Off");
        if (connected.length === 1)
            return connected[0].name;
        if (connected.length > 1)
            return connected.length + " devices";
        return I18n.tr("On");
    }

    function toggle(): void {
        if (adapter)
            adapter.enabled = !adapter.enabled;
    }

    // Pair, then trust and connect once BlueZ reports the device as paired.
    function pair(device: BluetoothDevice): void {
        failedDevice = null;
        pairingDevice = device;
        device.pair();
    }

    function deviceIcon(device: BluetoothDevice): string {
        const icon = device.icon;
        if (icon.startsWith("audio-headset") || icon.startsWith("audio-headphones"))
            return "headphones";
        if (icon.startsWith("audio"))
            return "speaker";
        if (icon.startsWith("input-keyboard"))
            return "keyboard";
        if (icon.startsWith("input-mouse"))
            return "mouse";
        if (icon.startsWith("input-gaming"))
            return "sports_esports";
        if (icon.startsWith("phone"))
            return "smartphone";
        if (icon.startsWith("computer"))
            return "computer";
        return "bluetooth";
    }

    function stateText(device: BluetoothDevice): string {
        switch (device.state) {
        case BluetoothDeviceState.Connecting:
            return I18n.tr("Connecting…");
        case BluetoothDeviceState.Disconnecting:
            return I18n.tr("Disconnecting…");
        case BluetoothDeviceState.Connected:
            return device.batteryAvailable ? I18n.tr("Connected · %1", Math.round(device.battery * 100) + "%") : I18n.tr("Connected");
        default:
            if (device.pairing)
                return I18n.tr("Pairing…");
            if (device === failedDevice)
                return I18n.tr("Pairing failed");
            return device.paired || device.bonded ? I18n.tr("Paired") : device.address;
        }
    }

    Binding {
        when: root.adapter !== null
        target: root.adapter
        property: "discovering"
        value: root.scanning && root.enabled
    }

    Connections {
        target: root.pairingDevice

        function onPairedChanged(): void {
            const device = root.pairingDevice;
            if (!device?.paired)
                return;
            device.trusted = true;
            device.connect();
            root.pairingDevice = null;
        }

        function onPairingChanged(): void {
            const device = root.pairingDevice;
            // Pairing ended without success.
            if (device && !device.pairing && !device.paired) {
                root.failedDevice = device;
                root.pairingDevice = null;
            }
        }
    }
}
