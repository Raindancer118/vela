//! Settings pages for Hyprland itself (Hyprland section of the sidebar).

use super::binder::group;
use super::hypr_rows::Rows;
use adw::prelude::*;

fn page(r: &Rows) -> adw::PreferencesPage {
    let p = adw::PreferencesPage::new();
    if r.store.available() {
        p.set_description("Changes apply at once. Reset (↶) brings back the value from your hyprland.lua.");
    } else {
        p.set_description("Hyprland is not reachable — these settings need a running Hyprland session.");
        p.set_sensitive(false);
    }
    p
}

pub fn windows(r: &Rows) -> adw::PreferencesPage {
    let p = page(r);

    let gaps = group("Gaps", "");
    gaps.add(&r.edge_gaps());
    gaps.add(&r.gaps_slider("general:gaps_in", "Gaps between windows", "Space between tiled windows", 40.0));
    gaps.add(&r.gaps_slider(
        "general:float_gaps",
        "Gaps for floating windows",
        "Space to the screen edges for floating windows",
        40.0,
    ));
    gaps.add(&r.slider("general:gaps_workspaces", "Gaps between workspaces", "", 0.0, 100.0, 1.0, 0, " px"));
    p.add(&gaps);

    let borders = group("Borders", "");
    borders.add(&r.slider(
        "general:border_size",
        "Border width",
        "Thickness of the line around windows",
        0.0,
        20.0,
        1.0,
        0,
        " px",
    ));
    borders.add(&r.color("general:col.active_border", "Active window", "Border of the focused window"));
    borders.add(&r.color("general:col.inactive_border", "Other windows", "Border of every other window"));
    borders.add(&r.switch("decoration:border_part_of_window", "Border is part of the window", ""));
    let resize = r.expander(
        "general:resize_on_border",
        "Resize by dragging the border",
        "Drag borders and gaps to resize windows",
    );
    resize.add_row(&r.slider(
        "general:extend_border_grab_area",
        "Grab area",
        "Extra space around the border that still counts",
        0.0,
        50.0,
        1.0,
        0,
        " px",
    ));
    resize.add_row(&r.switch("general:hover_icon_on_border", "Resize cursor on borders", ""));
    borders.add(&resize);
    p.add(&borders);

    let layout = group("Layout", "");
    layout.add(&r.choice_str(
        "general:layout",
        "Tiling layout",
        "How windows are arranged",
        &[
            ("dwindle", "Dwindle (split in halves)"),
            ("master", "Master and stack"),
            ("scrolling", "Scrolling columns"),
            ("monocle", "Monocle (one at a time)"),
        ],
    ));
    layout.add(&r.switch(
        "general:no_focus_fallback",
        "No focus fallback",
        "Moving focus in a direction stops at the last window",
    ));
    p.add(&layout);

    let snap = group("Floating windows", "");
    let s = r.expander(
        "general:snap:enabled",
        "Snap to edges",
        "Floating windows snap to other windows and the screen edge",
    );
    s.add_row(&r.slider("general:snap:window_gap", "Distance to windows", "", 0.0, 100.0, 1.0, 0, " px"));
    s.add_row(&r.slider("general:snap:monitor_gap", "Distance to the screen edge", "", 0.0, 100.0, 1.0, 0, " px"));
    s.add_row(&r.switch(
        "general:snap:border_overlap",
        "Borders overlap",
        "Only one border width between snapped windows",
    ));
    s.add_row(&r.switch("general:snap:respect_gaps", "Keep the gaps", "Snapped windows keep the configured gaps"));
    snap.add(&s);
    snap.add(&r.choice(
        "general:resize_corner",
        "Resize from corner",
        "Corner a floating window resizes from",
        &[
            (0, "Nearest to the pointer"),
            (1, "Top left"),
            (2, "Top right"),
            (3, "Bottom right"),
            (4, "Bottom left"),
        ],
    ));
    p.add(&snap);

    let more = group("More", "");
    more.add(&r.switch("general:allow_tearing", "Allow tearing", "Lets games that ask for it skip vsync (less latency)"));
    more.add(&r.switch(
        "general:modal_parent_blocking",
        "Dialogs block their window",
        "The parent of a dialog takes no input",
    ));
    p.add(&more);
    p
}

pub fn effects(r: &Rows) -> adw::PreferencesPage {
    let p = page(r);

    let shape = group("Corners", "");
    shape.add(&r.slider("decoration:rounding", "Corner radius", "Rounded window corners", 0.0, 20.0, 1.0, 0, " px"));
    shape.add(&r.slider(
        "decoration:rounding_power",
        "Corner shape",
        "2 is a circle; higher values are squarer (squircle)",
        2.0,
        10.0,
        0.1,
        1,
        "",
    ));
    p.add(&shape);

    let opacity = group("Transparency", "");
    opacity.add(&r.slider("decoration:active_opacity", "Focused window", "", 0.1, 1.0, 0.01, 2, "%"));
    opacity.add(&r.slider("decoration:inactive_opacity", "Other windows", "", 0.1, 1.0, 0.01, 2, "%"));
    opacity.add(&r.slider("decoration:fullscreen_opacity", "Fullscreen windows", "", 0.1, 1.0, 0.01, 2, "%"));
    let dim = r.expander("decoration:dim_inactive", "Dim other windows", "Darken every window but the focused one");
    dim.add_row(&r.slider("decoration:dim_strength", "Strength", "", 0.0, 1.0, 0.01, 2, "%"));
    opacity.add(&dim);
    opacity.add(&r.slider(
        "decoration:dim_special",
        "Dim behind special workspaces",
        "How dark the rest gets while a scratchpad is open",
        0.0,
        1.0,
        0.01,
        2,
        "%",
    ));
    opacity.add(&r.switch("decoration:dim_modal", "Dim behind dialogs", "Darken the window a dialog belongs to"));
    p.add(&opacity);

    let blur = group("Blur", "Frosted glass behind translucent windows, panels and popups.");
    let b = r.expander("decoration:blur:enabled", "Blur", "");
    b.set_expanded(true);
    b.add_row(&r.slider("decoration:blur:size", "Blurriness", "Radius of the blur", 1.0, 30.0, 1.0, 0, ""));
    b.add_row(&r.slider(
        "decoration:blur:passes",
        "Passes",
        "More passes look smoother and cost more GPU",
        1.0,
        6.0,
        1.0,
        0,
        "",
    ));
    b.add_row(&r.slider("decoration:blur:brightness", "Brightness", "", 0.0, 2.0, 0.01, 2, ""));
    b.add_row(&r.slider("decoration:blur:contrast", "Contrast", "", 0.0, 2.0, 0.01, 2, ""));
    b.add_row(&r.slider(
        "decoration:blur:vibrancy",
        "Vibrancy",
        "Saturation of the blurred colours",
        0.0,
        1.0,
        0.01,
        2,
        "",
    ));
    b.add_row(&r.slider("decoration:blur:vibrancy_darkness", "Vibrancy on dark colours", "", 0.0, 1.0, 0.01, 2, ""));
    b.add_row(&r.slider("decoration:blur:noise", "Noise", "Grain against colour banding", 0.0, 0.2, 0.001, 3, ""));
    b.add_row(&r.switch("decoration:blur:xray", "X-ray", "Floating windows blur the wallpaper, not the windows below"));
    b.add_row(&r.switch(
        "decoration:blur:ignore_opacity",
        "Ignore opacity",
        "Blur fully even behind mostly opaque windows",
    ));
    b.add_row(&r.switch("decoration:blur:popups", "Blur popups", "Menus and tooltips"));
    b.add_row(&r.slider(
        "decoration:blur:popups_ignorealpha",
        "Popup threshold",
        "Popup pixels more transparent than this stay unblurred",
        0.0,
        1.0,
        0.01,
        2,
        "",
    ));
    b.add_row(&r.switch("decoration:blur:special", "Blur behind special workspaces", "Expensive"));
    b.add_row(&r.switch("decoration:blur:input_methods", "Blur input method popups", ""));
    b.add_row(&r.switch("decoration:blur:new_optimizations", "Optimisations", "Leave on unless the blur looks wrong"));
    blur.add(&b);
    p.add(&blur);

    let shadow = group("Shadow", "");
    let s = r.expander("decoration:shadow:enabled", "Drop shadow", "");
    s.add_row(&r.slider("decoration:shadow:range", "Size", "", 0.0, 100.0, 1.0, 0, " px"));
    s.add_row(&r.slider("decoration:shadow:render_power", "Falloff", "Higher fades out faster", 1.0, 4.0, 1.0, 0, ""));
    s.add_row(&r.slider("decoration:shadow:scale", "Scale", "", 0.0, 1.0, 0.01, 2, ""));
    s.add_row(&r.switch("decoration:shadow:sharp", "Sharp", "Hard edge instead of a soft falloff"));
    s.add_row(&r.color("decoration:shadow:color", "Colour", "Alpha sets how strong it is"));
    s.add_row(&r.color("decoration:shadow:color_inactive", "Colour of other windows", ""));
    shadow.add(&s);
    p.add(&shadow);

    let glow = group("Glow", "");
    let g = r.expander("decoration:glow:enabled", "Inner glow", "Soft light along the inside of window edges");
    g.add_row(&r.slider("decoration:glow:range", "Size", "", 0.0, 100.0, 1.0, 0, " px"));
    g.add_row(&r.slider("decoration:glow:render_power", "Falloff", "", 1.0, 4.0, 1.0, 0, ""));
    g.add_row(&r.color("decoration:glow:color", "Colour", ""));
    g.add_row(&r.color("decoration:glow:color_inactive", "Colour of other windows", ""));
    glow.add(&g);
    p.add(&glow);
    p
}
