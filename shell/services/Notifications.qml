pragma Singleton

import QtQuick
import Quickshell
import Quickshell.Services.Notifications
import qs

// Notification daemon (org.freedesktop.Notifications). All notifications
// are kept in the panel list; `popups` holds the ones currently shown as
// popups while the panel is closed.
Singleton {
    id: root

    // Newest first.
    readonly property var list: [...server.trackedNotifications.values].reverse()
    readonly property int count: list.length
    // App names in order of their newest notification (for grouping).
    readonly property var apps: [...new Set(list.map(n => appKey(n)))]

    property var popups: []
    readonly property bool dnd: Persist.doNotDisturb

    // Arrival times (id -> ms since epoch).
    property var times: ({})
    // Ticks while the panel or a popup is visible, for "5 min ago" labels.
    property real now: Date.now()

    function appKey(n: Notification): string {
        return n.appName || I18n.tr("Unknown");
    }

    function timeOf(n: Notification): real {
        return times[n.id] ?? now;
    }

    function toggleDnd(): void {
        Persist.doNotDisturb = !Persist.doNotDisturb;
        if (Persist.doNotDisturb)
            clearPopups();
    }

    function hidePopup(n: Notification): void {
        popups = popups.filter(p => p !== n);
        if (n.transient)
            n.expire();
    }

    function clearPopups(): void {
        const transients = popups.filter(p => p.transient);
        popups = [];
        transients.forEach(p => p.expire());
    }

    function dismiss(n: Notification): void {
        n.dismiss();
    }

    function dismissApp(app: string): void {
        list.filter(n => appKey(n) === app).forEach(n => n.dismiss());
    }

    function clearAll(): void {
        dismissMany(list);
    }

    function dismissMany(ns: var): void {
        // Copy: dismissing shrinks trackedNotifications (and thus `list`).
        [...ns].forEach(n => n.dismiss());
    }

    // Default action if the app offers one.
    function activate(n: Notification): void {
        const action = n.actions.find(a => a.identifier === "default");
        if (action)
            action.invoke();
        else
            hidePopup(n);
    }

    function relativeTime(n: Notification): string {
        const seconds = Math.max(0, (now - timeOf(n)) / 1000);
        if (seconds < 24 * 3600)
            return I18n.relativeTime(seconds);
        return Config.locale.toString(new Date(timeOf(n)), Config.shortDateFormat);
    }

    // A new notification, or one that replaced an earlier one (same id).
    function arrived(n: Notification): void {
        times = Object.assign({}, times, {
            [n.id]: Date.now()
        });
        now = Date.now();
        saveTimes.restart();

        if (dnd || ShellState.panelOpen) {
            // Transient notifications only exist as popups.
            if (n.transient)
                n.expire();
            return;
        }
        const shown = [n, ...popups.filter(p => p !== n)];
        popups = shown.slice(0, Config.popupMaxVisible);
        shown.slice(Config.popupMaxVisible).filter(p => p.transient).forEach(p => p.expire());
    }

    function forget(n: Notification): void {
        popups = popups.filter(p => p !== n);
        const next = Object.assign({}, times);
        delete next[n.id];
        times = next;
        saveTimes.restart();
    }

    // Arrival times survive live reloads together with the notifications.
    // Stored as JSON because JS objects can't cross engines.
    PersistentProperties {
        id: state

        reloadableId: "notificationTimes"

        property string timesJson: "{}"

        onReloaded: root.times = JSON.parse(timesJson)
    }

    // Batches writes, e.g. "Clear all" with many notifications.
    Timer {
        id: saveTimes

        interval: 0
        onTriggered: state.timesJson = JSON.stringify(root.times)
    }

    Timer {
        interval: Config.relativeTimeInterval
        repeat: true
        running: ShellState.panelOpen || root.popups.length > 0
        triggeredOnStart: true
        onTriggered: root.now = Date.now()
    }

    NotificationServer {
        id: server

        keepOnReload: true
        persistenceSupported: true
        bodySupported: true
        bodyMarkupSupported: true
        bodyHyperlinksSupported: true
        actionsSupported: true
        imageSupported: true

        onNotification: n => {
            n.tracked = true;
            // Re-emitted after a live reload: keep, but don't pop up again.
            if (!n.lastGeneration)
                root.arrived(n);
        }
    }

    Connections {
        target: server.trackedNotifications

        function onObjectRemovedPost(object: QtObject, index: int): void {
            root.forget(object);
        }
    }

    // Replacements (replaces_id) update the existing object in place and
    // don't emit `notification` again; treat them as new arrivals.
    Instantiator {
        model: server.trackedNotifications

        Connections {
            required property Notification modelData

            target: modelData

            function onSummaryChanged(): void {
                Qt.callLater(root.arrived, modelData);
            }

            function onBodyChanged(): void {
                Qt.callLater(root.arrived, modelData);
            }
        }
    }
}
