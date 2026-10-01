import QtQuick
import qs

// The launcher's divider: a hairline that fades out at both ends.
Rectangle {
    implicitHeight: 1
    gradient: Gradient {
        orientation: Gradient.Horizontal

        GradientStop {
            position: 0
            color: Theme.withAlpha(Theme.colors.tint, 0)
        }

        GradientStop {
            position: 0.18
            color: Theme.colors.divider
        }

        GradientStop {
            position: 0.82
            color: Theme.colors.divider
        }

        GradientStop {
            position: 1
            color: Theme.withAlpha(Theme.colors.tint, 0)
        }
    }
}
