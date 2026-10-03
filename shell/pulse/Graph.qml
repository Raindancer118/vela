import QtQuick
import QtQuick.Shapes
import qs
import qs.components
import "Fmt.js" as Fmt

// Live area graph. With smooth graphs (Settings → Pulse) new points slide
// in from the right over one sample interval (only the plot moves, the path
// is rebuilt once per sample) and the scale eases to new maxima; otherwise
// the graph steps once per sample, like it is measured. Hovering shows the
// value under the cursor.
Item {
    id: root

    property var values: []
    // Optional second series (write, upload): a line without fill.
    property var values2: []
    property color color: Theme.pulse.cpu
    property color color2: Theme.pulse.diskWrite
    // Fixed top (e.g. 100 for percent); 0 = scale to the data.
    property real max: 0
    // Smallest automatic top, so idle graphs don't amplify noise.
    property real minMax: 1
    property int points: 60
    property bool grid: true
    property bool hoverable: true
    property bool fill: true
    property real lineWidth: Theme.pulse.graphLine
    property var format: v => Fmt.num(v, 0, Qt.locale())
    property string label2: ""
    property string label1: ""

    readonly property bool smooth: VelaConfig.pulse.smoothGraphs && Theme.anim.normal > 0
    readonly property real step: width / Math.max(1, points)
    readonly property real wantTop: max > 0 ? max : Fmt.niceMax(values.slice(-points - 2).concat(values2.slice(-points - 2)), minMax)
    property real scaleTop: wantTop
    Behavior on scaleTop {
        enabled: root.smooth
        Anim {
            duration: Theme.anim.slow
        }
    }

    clip: true

    onValuesChanged: {
        rebuild();
        slide.stop();
        if (smooth && width > 0) {
            plot.x = 0;
            slide.start();
        } else {
            plot.x = -step;
        }
    }
    onScaleTopChanged: rebuild()
    onWidthChanged: rebuild()
    onHeightChanged: rebuild()
    Component.onCompleted: rebuild()

    function coords(vals: var): var {
        const n = vals.length;
        const h = root.height;
        const pad = root.lineWidth;
        const out = [];
        const from = Math.max(0, n - root.points - 2);
        for (let i = from; i < n; i++) {
            const x = (i - (n - 1)) * root.step + root.width + root.step;
            const v = Math.max(0, Math.min(root.scaleTop, vals[i]));
            out.push(Qt.point(x, h - pad - v / root.scaleTop * (h - 2 * pad)));
        }
        return out;
    }

    function rebuild(): void {
        if (width <= 0 || height <= 0)
            return;
        const p = coords(values);
        line1.path = p;
        area.path = p.length > 0 ? [Qt.point(p[0].x, height)].concat(p, [Qt.point(p[p.length - 1].x, height)]) : [];
        line2.path = coords(values2);
    }

    // Grid: quarters.
    Repeater {
        model: root.grid ? 3 : 0

        Rectangle {
            required property int index

            width: root.width
            height: 1
            y: Math.round(root.height * (index + 1) / 4)
            color: Theme.pulse.graphGrid
        }
    }

    Item {
        id: plot

        width: root.width + root.step
        height: root.height
        x: -root.step

        NumberAnimation {
            id: slide

            target: plot
            property: "x"
            from: 0
            to: -root.step
            duration: Pulse.interval
        }

        Shape {
            anchors.fill: parent
            preferredRendererType: Shape.CurveRenderer

            ShapePath {
                strokeWidth: 0
                strokeColor: "transparent"
                fillGradient: LinearGradient {
                    y1: 0
                    y2: root.height
                    GradientStop {
                        position: 0
                        color: Theme.withAlpha(root.color, root.fill ? Theme.pulse.graphFill * 1.6 : 0)
                    }
                    GradientStop {
                        position: 1
                        color: Theme.withAlpha(root.color, 0)
                    }
                }
                PathPolyline {
                    id: area
                }
            }

            ShapePath {
                strokeWidth: root.lineWidth
                strokeColor: root.color
                fillColor: "transparent"
                joinStyle: ShapePath.RoundJoin
                capStyle: ShapePath.RoundCap
                PathPolyline {
                    id: line1
                }
            }

            ShapePath {
                strokeWidth: root.lineWidth
                strokeColor: root.values2.length > 0 ? root.color2 : "transparent"
                fillColor: "transparent"
                joinStyle: ShapePath.RoundJoin
                capStyle: ShapePath.RoundCap
                strokeStyle: ShapePath.DashLine
                dashPattern: [2, 2]
                PathPolyline {
                    id: line2
                }
            }
        }
    }

    // Value under the cursor.
    MouseArea {
        id: hover

        anchors.fill: parent
        hoverEnabled: root.hoverable
        acceptedButtons: Qt.NoButton
        readonly property int back: Math.max(0, Math.round((root.width - mouseX) / root.step))
        readonly property int idx: root.values.length - 1 - back
    }

    Item {
        visible: hover.containsMouse && hover.idx >= 0
        anchors.fill: parent

        Rectangle {
            x: root.width - hover.back * root.step
            width: 1
            height: parent.height
            color: Theme.withAlpha(Theme.colors.text, 0.25)
        }

        Rectangle {
            readonly property real v: root.values[hover.idx] ?? 0
            x: root.width - hover.back * root.step - width / 2
            y: root.height - root.lineWidth - Math.min(root.scaleTop, v) / root.scaleTop * (root.height - 2 * root.lineWidth) - height / 2
            width: 8
            height: 8
            radius: 4
            color: root.color
            border.width: 2
            border.color: Theme.colors.background
        }

        Rectangle {
            id: tip

            readonly property real tx: root.width - hover.back * root.step
            x: Math.max(4, Math.min(root.width - width - 4, tx + (tx > root.width / 2 ? -width - 10 : 10)))
            y: 4
            width: tipText.implicitWidth + 2 * Theme.spacing.sm
            height: tipText.implicitHeight + Theme.spacing.xs * 2
            radius: Theme.radius.small
            color: Theme.withAlpha(Theme.colors.background, 0.92)
            border.width: 1
            border.color: Theme.colors.outline

            StyledText {
                id: tipText

                anchors.centerIn: parent
                font.pixelSize: Theme.font.small
                font.features: { "tnum": 1 }
                text: {
                    const secs = hover.back * Pulse.interval / 1000;
                    const ago = secs < 1 ? I18n.tr("now") : I18n.tr("%1 s ago", Math.round(secs));
                    const v1 = (root.label1 ? root.label1 + " " : "") + root.format(root.values[hover.idx] ?? 0);
                    const i2 = root.values2.length - 1 - hover.back;
                    const v2 = root.values2.length > 0 && i2 >= 0 ? "   " + (root.label2 ? root.label2 + " " : "") + root.format(root.values2[i2]) : "";
                    return v1 + v2 + "  ·  " + ago;
                }
            }
        }
    }
}
