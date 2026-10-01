// Share picker: layout/navigation helpers and that the picker loads.
//   qs -p test-sharepicker.qml   (scripts/shell-test.sh); prints PASS or FAIL
import QtQuick
import Quickshell
import qs
import qs.sharepicker
import "sharepicker/logic.js" as Logic

ShellRoot {
    Component.onCompleted: {
        const errors = [];
        const check = (name, got, want) => {
            if (JSON.stringify(got) !== JSON.stringify(want))
                errors.push(name + ": " + JSON.stringify(got) + " != " + JSON.stringify(want));
        };

        // Three monitors in a row, the last one smaller: aligned at the top.
        const desk = [
            { name: "A", x: 0, y: 0, width: 1920, height: 1200 },
            { name: "B", x: 1920, y: 0, width: 1920, height: 1200 },
            { name: "C", x: 3840, y: 0, width: 1600, height: 900 }
        ];
        const rects = Logic.screenLayout(desk, 544, 400, 0);
        check("layout A", rects[0], { name: "A", x: 0, y: 140, width: 192, height: 120 });
        check("layout C", rects[2], { name: "C", x: 384, y: 140, width: 160, height: 90 });
        check("layout gap", Logic.screenLayout([desk[0]], 192, 120, 10)[0], { name: "A", x: 5, y: 5, width: 182, height: 110 });
        check("layout empty", Logic.screenLayout([], 100, 100, 0), []);

        check("nearest right", Logic.nearestScreen(rects, 0, "right"), 1);
        check("nearest left edge", Logic.nearestScreen(rects, 0, "left"), 0);
        check("nearest from none", Logic.nearestScreen(rects, -1, "left"), 0);

        const wins = [
            { title: "Overview - Brave", className: "brave-browser", workspaceId: 2 },
            { title: "Downloads", className: "org.kde.dolphin", workspaceId: -98 },
            { title: "Terminal", className: "ghostty", workspaceId: 1 }
        ];
        check("filter words", Logic.filterWindows(wins, "  brave over ").map(w => w.title), ["Overview - Brave"]);
        check("filter class", Logic.filterWindows(wins, "DOLPHIN").length, 1);
        check("filter none", Logic.filterWindows(wins, "").length, 3);
        check("sort", Logic.sortWindows(wins).map(w => w.workspaceId), [1, 2, -98]);

        check("grid right", Logic.gridMove(0, 5, 3, "right"), 1);
        check("grid right edge", Logic.gridMove(2, 5, 3, "right"), 2);
        check("grid down", Logic.gridMove(1, 5, 3, "down"), 4);
        check("grid down past end", Logic.gridMove(2, 5, 3, "down"), 2);
        check("grid up top", Logic.gridMove(1, 5, 3, "up"), 1);
        check("grid from none", Logic.gridMove(-1, 5, 3, "down"), 0);
        check("grid empty", Logic.gridMove(0, 0, 3, "down"), -1);

        check("columns fit", Logic.gridColumns(5, 900, 480, 0.62, 50, 3, 5), 3);
        check("columns more", Logic.gridColumns(8, 900, 480, 0.62, 50, 3, 5), 4);
        check("columns max", Logic.gridColumns(40, 900, 400, 0.62, 50, 3, 5), 5);
        check("title built-in", Logic.screenTitle("eDP-1", "0x150C", "Built-in"), "Built-in");
        check("title model", Logic.screenTitle("DP-6", "HP E243i", "Built-in"), "HP E243i");
        check("title fallback", Logic.screenTitle("HDMI-A-1", "", "Built-in"), "HDMI-A-1");

        check("answer screen", JSON.parse(Logic.answer("screen", "DP-6", true)), { kind: "screen", token: true, output: "DP-6" });
        check("answer window", JSON.parse(Logic.answer("window", 42, false)), { kind: "window", token: false, handle: 42 });

        // The picker window itself (not shown: no VELA_SHARE_OUT here).
        const c = Qt.createComponent("sharepicker/SharePicker.qml");
        if (c.status !== Component.Ready)
            errors.push("SharePicker: " + c.errorString());

        console.log(errors.length === 0 ? "PASS" : "FAIL " + errors.join("; "));
        Qt.callLater(Qt.quit);
    }
}
