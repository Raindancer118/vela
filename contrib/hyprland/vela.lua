-- vela integration for Hyprland (Lua config, Hyprland >= 0.55).
--
-- Usage in ~/.config/hypr/hyprland.lua:
--   dofile(os.getenv("HOME") .. "/.config/hypr/vela.lua").setup()
--
-- Tapping Super (pressing and releasing it without any other key in between)
-- toggles the launcher. Super+Q, Super+1, … keep working and never open it,
-- also for combinations that have no bind. Holding Super longer than
-- `tap_ms` (e.g. while dragging a window with Super+mouse) is no tap either.

local M = {}

local function exists(path)
    local f = io.open(path, "r")
    if f then f:close() return true end
    return false
end

local function find_binary()
    for _, p in ipairs({ os.getenv("HOME") .. "/.local/bin/vela", "/usr/local/bin/vela", "/usr/bin/vela" }) do
        if exists(p) then return p end
    end
    return "vela"
end

local defaults = {
    -- Path of the vela binary; found automatically when nil.
    binary = nil,
    -- Command run on a Super tap; defaults to "<binary> toggle".
    command = nil,
    -- XKB keycodes of Super_L / Super_R on standard (evdev) keyboards.
    keycodes = { 133, 134 },
    tap_ms = 400,
    -- Start the vela daemon with Hyprland (as systemd user service if installed).
    autostart = true,
    -- Blur behind the launcher; vela animates itself, so Hyprland doesn't.
    blur = true,
    ignore_alpha = 0.3,
    -- Start the control center (`vela shell`, needs Quickshell) with Hyprland.
    -- Don't also start `qs` yourself: two notification daemons would fight.
    shell = true,
    -- Run hypridle with vela's [idle] settings (`vela idle`). Don't also
    -- start hypridle yourself.
    idle = true,
}

function M.setup(opts)
    local o = {}
    for k, v in pairs(defaults) do o[k] = v end
    for k, v in pairs(opts or {}) do o[k] = v end
    local bin = o.binary or find_binary()
    local command = o.command or (bin .. " toggle")

    local is_super = {}
    for _, code in ipairs(o.keycodes) do is_super[code] = true end

    local armed, press_id, expire = false, 0, nil
    -- Compositor-side clock: a press held longer than tap_ms is no tap.
    -- (Key event timestamps come from the client and can't be trusted for
    -- virtual keyboards.) A fired oneshot timer can't be re-armed, so every
    -- press gets its own timer; the id guards against stale callbacks.
    local function stop_timer()
        if expire then expire:set_enabled(false) end
        expire = nil
    end

    -- The event fires for every key before binds are processed.
    -- state: 0 = released, 1 = pressed, 2 = repeated
    hl.on("input.keyboard.key", function(keycode, _, state)
        if is_super[keycode] then
            if state == 1 then
                armed = true
                press_id = press_id + 1
                local id = press_id
                stop_timer()
                expire = hl.timer(function()
                    if id == press_id then armed = false end
                end, { timeout = o.tap_ms, type = "oneshot" })
            elseif state == 0 then
                stop_timer()
                if armed then
                    hl.exec_cmd(command)
                end
                armed = false
            end
        elseif state ~= 0 then
            -- Any other key while Super is held turns it into a shortcut.
            armed = false
        end
    end)

    hl.layer_rule({
        name         = "vela",
        match        = { namespace = "^vela$" },
        blur         = o.blur,
        ignore_alpha = o.ignore_alpha,
        no_anim      = true,
    })

    -- Optional blurred backdrop (Settings → Appearance); Hyprland fades it.
    hl.layer_rule({
        name  = "vela-backdrop",
        match = { namespace = "^vela-backdrop$" },
        blur  = o.blur,
    })

    -- Control center: translucent panel, popups and workspace dots get
    -- blurred behind their visible parts; they animate themselves.
    hl.layer_rule({
        name         = "vela-shell",
        match        = { namespace = "^quickshell-(panel|notifications|osd)$" },
        blur         = o.blur,
        ignore_alpha = o.ignore_alpha,
        no_anim      = true,
    })

    -- Optional backdrop behind the panel (Settings → Appearance → Blur).
    hl.layer_rule({
        name  = "vela-shell-backdrop",
        match = { namespace = "^vela-shell-backdrop$" },
        blur  = o.blur,
    })

    if o.idle then
        hl.on("hyprland.start", function()
            hl.exec_cmd(bin .. " idle")
        end)
    end

    if o.shell then
        hl.on("hyprland.start", function()
            hl.exec_cmd(bin .. " shell")
        end)
    end

    if o.autostart then
        hl.on("hyprland.start", function()
            -- The service needs this session's Wayland environment; restart
            -- so a daemon from an earlier session never lingers.
            hl.exec_cmd("systemctl --user import-environment WAYLAND_DISPLAY HYPRLAND_INSTANCE_SIGNATURE XDG_CURRENT_DESKTOP XDG_SESSION_TYPE DISPLAY"
                .. " && systemctl --user restart vela.service 2>/dev/null || " .. bin .. " daemon")
        end)
    end

    return M
end

return M
