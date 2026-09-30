import QtQuick
import qs

// Settles with a slight overshoot.
NumberAnimation {
    duration: Theme.anim.slow
    easing.type: Easing.OutBack
    easing.overshoot: Theme.anim.springOvershoot
}
