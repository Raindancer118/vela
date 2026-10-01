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

pub fn input(r: &Rows) -> adw::PreferencesPage {
    let p = page(r);

    let kb = group(
        "Keyboard",
        "XKB names, e.g. layout “de” or “us,de” with options “grp:alt_shift_toggle”. Press Enter to apply.",
    );
    kb.add(&r.entry("input:kb_layout", "Layout"));
    kb.add(&r.entry("input:kb_variant", "Variant"));
    kb.add(&r.entry("input:kb_options", "Options"));
    kb.add(&r.slider(
        "input:repeat_rate",
        "Repeat rate",
        "Repeats per second while a key is held",
        5.0,
        80.0,
        1.0,
        0,
        " /s",
    ));
    kb.add(&r.slider(
        "input:repeat_delay",
        "Repeat delay",
        "Time before a held key repeats",
        100.0,
        1000.0,
        10.0,
        0,
        " ms",
    ));
    kb.add(&r.switch("input:numlock_by_default", "Num Lock on at start", ""));
    kb.add(&r.switch(
        "input:resolve_binds_by_sym",
        "Shortcuts by symbol",
        "With several layouts, shortcuts follow the symbol instead of the key position",
    ));
    p.add(&kb);

    let mouse = group("Mouse", "");
    mouse.add(&r.slider("input:sensitivity", "Pointer speed", "0 leaves it unchanged", -1.0, 1.0, 0.05, 2, ""));
    mouse.add(&r.choice_str(
        "input:accel_profile",
        "Acceleration",
        "",
        &[("", "Device default"), ("adaptive", "Adaptive"), ("flat", "Flat (no acceleration)")],
    ));
    mouse.add(&r.switch("input:natural_scroll", "Natural scrolling", "Content moves with the wheel"));
    mouse.add(&r.slider("input:scroll_factor", "Scroll speed", "", 0.1, 2.0, 0.05, 2, "×"));
    mouse.add(&r.switch("input:left_handed", "Left-handed", "Swap the left and right button"));
    p.add(&mouse);

    let tp = group("Touchpad", "");
    tp.add(&r.switch("input:touchpad:natural_scroll", "Natural scrolling", ""));
    tp.add(&r.slider("input:touchpad:scroll_factor", "Scroll speed", "", 0.1, 2.0, 0.05, 2, "×"));
    tp.add(&r.switch(
        "input:touchpad:tap-to-click",
        "Tap to click",
        "1, 2 or 3 fingers for left, right and middle click",
    ));
    tp.add(&r.switch("input:touchpad:tap-and-drag", "Tap and drag", ""));
    tp.add(&r.choice(
        "input:touchpad:drag_lock",
        "Drag lock",
        "Keep dragging after lifting the finger",
        &[(0, "Off"), (1, "With timeout"), (2, "Sticky")],
    ));
    tp.add(&r.switch("input:touchpad:disable_while_typing", "Off while typing", ""));
    tp.add(&r.switch(
        "input:touchpad:clickfinger_behavior",
        "Click with fingers",
        "Click with 1, 2 or 3 fingers instead of touchpad areas",
    ));
    tp.add(&r.switch("input:touchpad:middle_button_emulation", "Middle click by both buttons", ""));
    tp.add(&r.choice(
        "input:touchpad:drag_3fg",
        "Three-finger drag",
        "",
        &[(0, "Off"), (1, "Three fingers"), (2, "Four fingers")],
    ));
    p.add(&tp);

    let gestures = group("Workspace swipe", "Swipe with several fingers to switch workspaces.");
    gestures.add(&r.switch("gestures:workspace_swipe_invert", "Invert direction", ""));
    gestures.add(&r.slider(
        "gestures:workspace_swipe_distance",
        "Distance",
        "How far one swipe goes",
        100.0,
        1000.0,
        10.0,
        0,
        " px",
    ));
    gestures.add(&r.slider(
        "gestures:workspace_swipe_cancel_ratio",
        "Commit at",
        "How far to swipe before it switches",
        0.0,
        1.0,
        0.05,
        2,
        "%",
    ));
    gestures.add(&r.switch(
        "gestures:workspace_swipe_create_new",
        "New workspace at the end",
        "Swiping past the last workspace makes a new one",
    ));
    gestures.add(&r.switch(
        "gestures:workspace_swipe_forever",
        "Swipe past neighbours",
        "Keep going instead of stopping at the next workspace",
    ));
    gestures.add(&r.switch("gestures:workspace_swipe_touch", "On touchscreens", "Swipe from the screen edge"));
    p.add(&gestures);

    let cursor = group("Cursor", "");
    cursor.add(&r.slider(
        "cursor:inactive_timeout",
        "Hide when idle",
        "Seconds without movement; 0 never hides it",
        0.0,
        20.0,
        1.0,
        0,
        " s",
    ));
    cursor.add(&r.switch("cursor:hide_on_key_press", "Hide while typing", "Until the mouse moves"));
    cursor.add(&r.switch("cursor:no_warps", "Never move the cursor", "Hyprland won't jump the pointer to focused windows"));
    cursor.add(&r.choice(
        "cursor:warp_on_change_workspace",
        "Jump to the window on workspace change",
        "",
        &[(0, "No"), (1, "Yes"), (2, "Always, even with no_warps")],
    ));
    cursor.add(&r.slider("cursor:zoom_factor", "Zoom", "Magnify around the cursor; 1 is off", 1.0, 5.0, 0.1, 1, "×"));
    cursor.add(&r.switch("cursor:zoom_rigid", "Rigid zoom", "The zoomed view follows the cursor exactly"));
    p.add(&cursor);
    p
}

const WINDOW_STYLES: [(&str, &str); 6] = [
    ("", "Default"),
    ("slide", "Slide"),
    ("popin 87%", "Pop in"),
    ("popin 60%", "Pop in, strong"),
    ("gnomed", "Gnome"),
    ("slide bottom", "Slide from the bottom"),
];
const LAYER_STYLES: [(&str, &str); 4] = [("", "Default"), ("fade", "Fade"), ("slide", "Slide"), ("popin 90%", "Pop in")];
const WORKSPACE_STYLES: [(&str, &str); 6] = [
    ("", "Default (slide)"),
    ("slide", "Slide"),
    ("slidevert", "Slide vertically"),
    ("fade", "Fade"),
    ("slidefade 20%", "Slide and fade"),
    ("slidefadevert 20%", "Slide and fade vertically"),
];

pub fn animations(r: &Rows) -> adw::PreferencesPage {
    let p = page(r);

    let all = group(
        "Animations",
        "Duration: Hyprland's speed in milliseconds. Parts without their own settings follow the one above.",
    );
    all.add(&r.switch("animations:enabled", "Animations", "Turn every animation on or off"));
    all.add(&r.anim("global", "Everything else", "Default for all animations", &[]));
    all.add(&r.switch(
        "animations:workspace_wraparound",
        "Wrap around",
        "Slide the right way between the first and last workspace",
    ));
    all.add(&r.switch(
        "misc:animate_manual_resizes",
        "Animate resizing",
        "When resizing windows with the mouse or keyboard",
    ));
    all.add(&r.switch("misc:animate_mouse_windowdragging", "Animate dragging", "Windows follow the mouse smoothly"));
    p.add(&all);

    let windows = group("Windows", "");
    windows.add(&r.anim("windows", "Windows", "Moving and resizing", &WINDOW_STYLES));
    windows.add(&r.anim("windowsIn", "Opening", "", &WINDOW_STYLES));
    windows.add(&r.anim("windowsOut", "Closing", "", &WINDOW_STYLES));
    windows.add(&r.anim("windowsMove", "Moving", "", &[]));
    p.add(&windows);

    let ws = group("Workspaces", "");
    ws.add(&r.anim("workspaces", "Switching workspaces", "", &WORKSPACE_STYLES));
    ws.add(&r.anim("specialWorkspace", "Special workspace", "Scratchpads", &WORKSPACE_STYLES));
    p.add(&ws);

    let fade = group("Fading", "");
    fade.add(&r.anim("fade", "Fade", "Default for everything below", &[]));
    fade.add(&r.anim("fadeIn", "Windows appearing", "", &[]));
    fade.add(&r.anim("fadeOut", "Windows disappearing", "", &[]));
    fade.add(&r.anim("fadeSwitch", "Focus change", "Opacity of the focused window", &[]));
    fade.add(&r.anim("fadeDim", "Dimming", "", &[]));
    fade.add(&r.anim("fadeShadow", "Shadows", "", &[]));
    fade.add(&r.anim("fadeLayers", "Panels and launchers", "", &[]));
    fade.add(&r.anim("fadePopups", "Popups", "", &[]));
    p.add(&fade);

    let other = group("More", "");
    other.add(&r.anim(
        "layers",
        "Panels and launchers",
        "Bars, notifications, launchers (vela animates itself)",
        &LAYER_STYLES,
    ));
    other.add(&r.anim("border", "Border colour", "", &[]));
    other.add(&r.anim(
        "borderangle",
        "Rotating border gradient",
        "Turns the gradient of borders around the window",
        &[],
    ));
    other.add(&r.anim("zoomFactor", "Cursor zoom", "", &[]));
    other.add(&r.anim("monitorAdded", "New monitor", "", &[]));
    p.add(&other);
    p
}

pub fn layouts(r: &Rows) -> adw::PreferencesPage {
    let p = page(r);

    let dwindle = group("Dwindle", "Every new window halves the focused one.");
    dwindle.add(&r.switch("dwindle:preserve_split", "Keep the split direction", "Don't flip it when windows get resized"));
    dwindle.add(&r.switch(
        "dwindle:smart_split",
        "Split where the pointer is",
        "Choose the side by the mouse position on the window",
    ));
    dwindle.add(&r.switch("dwindle:smart_resizing", "Resize where the pointer is", ""));
    dwindle.add(&r.choice(
        "dwindle:force_split",
        "New windows go",
        "",
        &[(0, "Where the mouse is"), (1, "Left / top"), (2, "Right / bottom")],
    ));
    dwindle.add(&r.slider("dwindle:default_split_ratio", "Split ratio", "1 splits evenly", 0.1, 1.9, 0.05, 2, ""));
    dwindle.add(&r.slider(
        "dwindle:split_width_multiplier",
        "Wide split threshold",
        "Split side by side when the window is this much wider than high",
        0.1,
        3.0,
        0.05,
        2,
        "×",
    ));
    dwindle.add(&r.slider("dwindle:special_scale_factor", "Special workspace size", "", 0.0, 1.0, 0.01, 2, "%"));
    p.add(&dwindle);

    let master = group("Master", "One big window and a stack of the others.");
    master.add(&r.choice_str(
        "master:orientation",
        "Master area",
        "",
        &[("left", "Left"), ("right", "Right"), ("top", "Top"), ("bottom", "Bottom"), ("center", "Centre")],
    ));
    master.add(&r.slider("master:mfact", "Master size", "", 0.05, 0.95, 0.01, 2, "%"));
    master.add(&r.choice_str(
        "master:new_status",
        "New windows become",
        "",
        &[("slave", "Part of the stack"), ("master", "Master"), ("inherit", "Like the focused window")],
    ));
    master.add(&r.switch("master:new_on_top", "New windows on top of the stack", ""));
    master.add(&r.switch("master:smart_resizing", "Resize where the pointer is", ""));
    master.add(&r.switch("master:drop_at_cursor", "Drop dragged windows at the cursor", ""));
    master.add(&r.switch("master:focus_master_on_close", "Focus the master after closing", ""));
    master.add(&r.slider("master:special_scale_factor", "Special workspace size", "", 0.0, 1.0, 0.01, 2, "%"));
    p.add(&master);

    let scrolling = group("Scrolling", "Columns on an endless strip, like niri or PaperWM.");
    scrolling.add(&r.slider(
        "scrolling:column_width",
        "Column width",
        "Share of the screen a new column takes",
        0.1,
        1.0,
        0.01,
        2,
        "%",
    ));
    scrolling.add(&r.switch("scrolling:fullscreen_on_one_column", "Single column fills the screen", ""));
    scrolling.add(&r.choice_str(
        "scrolling:direction",
        "New columns appear",
        "",
        &[("right", "Right"), ("left", "Left"), ("down", "Below"), ("up", "Above")],
    ));
    scrolling.add(&r.switch("scrolling:follow_focus", "Scroll to the focused window", ""));
    scrolling.add(&r.choice("scrolling:focus_fit_method", "Bring into view by", "", &[(0, "Centring"), (1, "Fitting")]));
    scrolling.add(&r.switch("scrolling:wrap_focus", "Focus wraps around", ""));
    p.add(&scrolling);

    let groups = group("Window groups (tabs)", "");
    groups.add(&r.switch("group:auto_group", "Add new windows to the group", "When the focused window is in a group"));
    groups.add(&r.switch("group:insert_after_current", "Insert after the current tab", ""));
    groups.add(&r.color("group:col.border_active", "Border of the active group", ""));
    groups.add(&r.color("group:col.border_inactive", "Border of other groups", ""));
    groups.add(&r.switch("group:groupbar:enabled", "Tab bar", "Bar with the windows of a group"));
    p.add(&groups);
    p
}

pub fn behaviour(r: &Rows) -> adw::PreferencesPage {
    let p = page(r);

    let focus = group("Focus", "");
    focus.add(&r.choice(
        "input:follow_mouse",
        "Focus follows the mouse",
        "",
        &[
            (0, "No, click to focus"),
            (1, "Yes"),
            (2, "Keyboard stays, mouse hovers"),
            (3, "Keyboard and mouse separate"),
        ],
    ));
    focus.add(&r.switch(
        "input:mouse_refocus",
        "Refocus on mouse move",
        "Off: only crossing a window border changes focus",
    ));
    focus.add(&r.choice(
        "input:focus_on_close",
        "After closing a window focus",
        "",
        &[(0, "The next window"), (1, "The window under the mouse"), (2, "The last used window")],
    ));
    focus.add(&r.switch("misc:focus_on_activate", "Apps may take the focus", "When an app asks to be focused"));
    focus.add(&r.switch("misc:mouse_move_focuses_monitor", "Mouse focuses monitors", ""));
    focus.add(&r.choice(
        "misc:on_focus_under_fullscreen",
        "Opening over a fullscreen window",
        "",
        &[(0, "Stay behind"), (1, "Take over fullscreen"), (2, "Leave fullscreen")],
    ));
    p.add(&focus);

    let ws = group("Workspaces", "");
    ws.add(&r.switch(
        "binds:workspace_back_and_forth",
        "Back and forth",
        "Switching to the current workspace goes to the previous one",
    ));
    ws.add(&r.switch("binds:allow_workspace_cycles", "Remember previous workspaces", ""));
    ws.add(&r.switch("binds:hide_special_on_workspace_change", "Hide scratchpads on workspace change", ""));
    ws.add(&r.switch("misc:close_special_on_empty", "Close empty scratchpads", ""));
    ws.add(&r.choice(
        "misc:initial_workspace_tracking",
        "Open apps where they were started",
        "",
        &[(0, "Off"), (1, "Only the first window"), (2, "Every window")],
    ));
    p.add(&ws);

    let screen = group("Screen", "");
    screen.add(&r.choice(
        "misc:vrr",
        "Variable refresh rate",
        "Adaptive sync (VRR)",
        &[(0, "Off"), (1, "On"), (2, "Fullscreen only"), (3, "Fullscreen games only")],
    ));
    screen.add(&r.switch("misc:disable_hyprland_logo", "No Hyprland logo", "Plain background where no wallpaper is drawn"));
    screen.add(&r.switch("misc:disable_splash_rendering", "No splash text", ""));
    screen.add(&r.color("misc:background_color", "Background colour", "Behind everything when there is no wallpaper"));
    screen.add(&r.switch("misc:mouse_move_enables_dpms", "Mouse wakes the screen", ""));
    screen.add(&r.switch("misc:key_press_enables_dpms", "Keys wake the screen", ""));
    p.add(&screen);

    let apps = group("Apps", "");
    let swallow = r.expander("misc:enable_swallow", "Swallow terminals", "A terminal hides while the app it started is open");
    swallow.add_row(&r.entry("misc:swallow_regex", "Terminal classes (regex)"));
    apps.add(&swallow);
    apps.add(&r.switch("misc:enable_anr_dialog", "Ask about hanging apps", "Dialog when an app stops responding"));
    apps.add(&r.switch("misc:middle_click_paste", "Middle click pastes", "The primary selection"));
    apps.add(&r.switch("xwayland:force_zero_scaling", "Unscaled X11 apps", "Sharp but small on scaled screens"));
    apps.add(&r.switch("xwayland:enabled", "X11 apps (XWayland)", ""));
    p.add(&apps);
    p
}
