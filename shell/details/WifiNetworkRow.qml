import QtQuick
import QtQuick.Layouts
import Quickshell.Networking
import qs
import qs.components
import qs.services

// A network in the Wi-Fi list. Unknown secured networks ask for a password
// inline; a rejected password asks again.
ColumnLayout {
    id: root

    required property WifiNetwork network
    property bool askPassword: false
    property string error: ""

    readonly property bool secured: NetworkState.isSecured(network)
    readonly property bool busy: network.stateChanging || network.state === ConnectionState.Connecting

    function activate(): void {
        error = "";
        if (network.known || !secured) {
            network.connect();
        } else if (NetworkState.usesPsk(network)) {
            askPassword = !askPassword;
            if (askPassword)
                Qt.callLater(() => password.input.forceActiveFocus());
        } else {
            error = I18n.tr("Enterprise networks need to be set up with nmtui");
        }
    }

    function submitPassword(): void {
        if (!NetworkState.validPsk(password.text)) {
            error = I18n.tr("Password must be 8 to 63 characters");
            return;
        }
        error = "";
        network.connectWithPsk(password.text);
        password.text = "";
        askPassword = false;
    }

    spacing: Theme.spacing.xs

    Connections {
        target: root.network

        function onConnectionFailed(reason: int): void {
            if (reason === ConnectionFailReason.NoSecrets && NetworkState.usesPsk(root.network)) {
                root.error = root.network.known ? I18n.tr("Wrong password") : "";
                root.askPassword = true;
                Qt.callLater(() => password.input.forceActiveFocus());
            } else {
                root.error = I18n.tr("Couldn't connect");
            }
        }
    }

    ListRow {
        Layout.fillWidth: true
        icon: NetworkState.signalIcon(root.network.signalStrength)
        title: root.network.name
        subtitle: {
            if (root.busy)
                return I18n.tr("Connecting…");
            if (root.error !== "")
                return root.error;
            const security = NetworkState.securityName(root.network);
            return root.network.known ? I18n.tr("Saved · %1", security) : security;
        }
        onClicked: root.activate()

        MaterialIcon {
            anchors.verticalCenter: parent.verticalCenter
            visible: root.secured
            icon: "lock"
            size: Theme.icon.small
            color: Theme.colors.textMuted
        }

        IconButton {
            anchors.verticalCenter: parent.verticalCenter
            visible: root.network.known
            implicitWidth: Theme.size.pillButton
            implicitHeight: Theme.size.pillButton
            tonal: false
            icon: "delete"
            iconSize: Theme.icon.small
            iconColor: Theme.colors.textMuted
            onClicked: root.network.forget()
        }
    }

    RowLayout {
        Layout.fillWidth: true
        Layout.leftMargin: Theme.spacing.md
        Layout.bottomMargin: Theme.spacing.sm
        visible: root.askPassword
        spacing: Theme.spacing.sm

        TextField {
            id: password

            Layout.fillWidth: true
            placeholder: I18n.tr("Password")
            password: true
            onAccepted: root.submitPassword()
        }

        PillButton {
            text: I18n.tr("Connect")
            style: "filled"
            enabled: NetworkState.validPsk(password.text)
            onClicked: root.submitPassword()
        }
    }
}
