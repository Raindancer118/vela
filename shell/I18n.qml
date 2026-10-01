pragma Singleton

import QtQuick
import Quickshell

// UI language follows LANGUAGE/LC_ALL/LC_MESSAGES/LANG, date and time formats follow
// LC_ALL/LC_TIME/LANG (gettext precedence). Languages without a table fall back to the
// English source strings.
Singleton {
    id: root

    function envLocale(names: var): string {
        for (const name of names) {
            const value = (Quickshell.env(name) ?? "").split(":")[0].split(".")[0].split("@")[0];
            if (value !== "")
                return value === "C" || value === "POSIX" ? "en_US" : value;
        }
        return "en_US";
    }

    readonly property string language: envLocale(["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"]).split("_")[0]
    readonly property var timeLocale: Qt.locale(envLocale(["LC_ALL", "LC_TIME", "LANG"]))

    // Drops the year from a locale date format ("dddd, d. MMMM yyyy" -> "dddd, d. MMMM").
    // A trailing "." stays because German writes "30.09.".
    function withoutYear(format: string): string {
        return format.replace(/[\s,\/-]*y+[\s,\/-]*/g, " ").trim().replace(/^[\s,\/-]+|[\s,\/-]+$/g, "");
    }

    function tr(text: string, ...args): string {
        let out = root.translations[root.language]?.[text] ?? text;
        args.forEach((arg, i) => out = out.replace("%" + (i + 1), arg));
        return out;
    }

    function relativeTime(seconds: real): string {
        if (seconds < 60)
            return tr("now");
        const minutes = Math.floor(seconds / 60);
        if (minutes < 60)
            return tr("%1 min ago", minutes);
        return tr("%1 h ago", Math.floor(minutes / 60));
    }

    function duration(seconds: real): string {
        const minutes = Math.round(seconds / 60);
        const h = Math.floor(minutes / 60);
        return h > 0 ? tr("%1 h %2 min", h, minutes % 60) : tr("%1 min", minutes);
    }

    function uptime(seconds: real): string {
        const d = Math.floor(seconds / 86400);
        const h = Math.floor(seconds % 86400 / 3600);
        const m = Math.floor(seconds % 3600 / 60);
        const parts = [];
        if (d > 0)
            parts.push(tr("%1d", d));
        if (d > 0 || h > 0)
            parts.push(tr("%1h", h));
        parts.push(tr("%1m", m));
        return tr("up %1", parts.join(" "));
    }

    readonly property var translations: ({
            "de": {
                "Toggle the control center": "Kontrollzentrum ein-/ausblenden",
                "now": "gerade eben",
                "%1 min ago": "vor %1 Min.",
                "%1 h ago": "vor %1 Std.",
                "%1 h %2 min": "%1 Std. %2 Min.",
                "%1 min": "%1 Min.",
                "up %1": "läuft seit %1",
                "%1d": "%1 T.",
                "%1h": "%1 Std.",
                "%1m": "%1 Min.",
                "Unknown": "Unbekannt",
                "Notifications": "Benachrichtigungen",
                "Clear all": "Alle löschen",
                "All caught up": "Alles erledigt",
                "Do not disturb is on. New notifications won't pop up.": "„Nicht stören“ ist an. Neue Benachrichtigungen poppen nicht auf.",
                "Do not disturb is on. New notifications will still show up here.": "„Nicht stören“ ist an. Neue Benachrichtigungen erscheinen trotzdem hier.",
                "No notifications": "Keine Benachrichtigungen",
                "Show less": "Weniger anzeigen",
                "Show %1 more": "%1 weitere anzeigen",
                "Log out": "Abmelden",
                "Log out now?": "Jetzt abmelden?",
                "Restart": "Neu starten",
                "Restart now?": "Jetzt neu starten?",
                "Shut down": "Herunterfahren",
                "Shut down now?": "Jetzt herunterfahren?",
                "Cancel": "Abbrechen",
                "Confirm": "Bestätigen",
                "Muted": "Stumm",
                "Power mode": "Energiemodus",
                "Default": "Standard",
                "Unavailable": "Nicht verfügbar",
                "Sleep automatically": "Automatisch schlafen",
                "After %1 min": "Nach %1 Min.",
                "Never": "Nie",
                "7 d": "7 T",
                "Not available": "Nicht verfügbar",
                "Longer battery life, less performance": "Längere Akkulaufzeit, weniger Leistung",
                "Normal performance and power use": "Normale Leistung und normaler Verbrauch",
                "Full performance, higher power use": "Volle Leistung, höherer Verbrauch",
                "Power saver": "Energiesparen",
                "Balanced": "Ausgewogen",
                "Performance": "Leistung",
                "Settings": "Einstellungen",
                "No device": "Kein Gerät",
                "Night light": "Nachtlicht",
                "Do not disturb": "Nicht stören",
                "On": "An",
                "Off": "Aus",
                "Sound": "Ton",
                "Output": "Ausgabe",
                "Input": "Eingabe",
                "No input devices": "Keine Eingabegeräte",
                "Applications": "Anwendungen",
                "Default": "Standard",
                "Wi-Fi": "WLAN",
                "No Wi-Fi adapter": "Kein WLAN-Adapter",
                "Wi-Fi is blocked": "WLAN ist blockiert",
                "Wi-Fi is off": "WLAN ist aus",
                "Check the hardware switch or airplane mode.": "Hardware-Schalter oder Flugmodus prüfen.",
                "Turn it on to see networks.": "Einschalten, um Netzwerke zu sehen.",
                "Available networks": "Verfügbare Netzwerke",
                "Searching for networks…": "Suche nach Netzwerken…",
                "Disconnect": "Trennen",
                "Connect": "Verbinden",
                "Password": "Passwort",
                "Enterprise networks need to be set up with nmtui": "Enterprise-Netzwerke bitte mit nmtui einrichten",
                "Password must be 8 to 63 characters": "Das Passwort muss 8 bis 63 Zeichen lang sein",
                "Wrong password": "Falsches Passwort",
                "Couldn't connect": "Verbindung fehlgeschlagen",
                "Saved · %1": "Gespeichert · %1",
                "Unavailable": "Nicht verfügbar",
                "Blocked by hardware switch": "Durch Hardware-Schalter blockiert",
                "Not connected": "Nicht verbunden",
                "Open": "Offen",
                "Secured": "Gesichert",
                "No Bluetooth adapter": "Kein Bluetooth-Adapter",
                "Bluetooth is blocked": "Bluetooth ist blockiert",
                "Bluetooth is off": "Bluetooth ist aus",
                "Check airplane mode (rfkill).": "Flugmodus prüfen (rfkill).",
                "Turn it on to connect devices.": "Einschalten, um Geräte zu verbinden.",
                "Paired devices": "Gekoppelte Geräte",
                "Available devices": "Verfügbare Geräte",
                "Searching for devices…": "Suche nach Geräten…",
                "Blocked": "Blockiert",
                "Connecting…": "Verbinde…",
                "Disconnecting…": "Trenne…",
                "Connected": "Verbunden",
                "Connected · %1": "Verbunden · %1",
                "Pairing…": "Koppeln…",
                "Pairing failed": "Koppeln fehlgeschlagen",
                "Paired": "Gekoppelt",
                "Fully charged": "Voll geladen",
                "Charging": "Lädt",
                "Charging · %1 until full": "Lädt · voll in %1",
                "Plugged in": "Angeschlossen",
                "%1 left": "Noch %1",
                "On battery": "Akkubetrieb"
            }
        })
}
