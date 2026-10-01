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

-- install.sh's selection (<data dir>/vela/components.toml); missing file or
-- key = installed. Only `key = true|false` lines matter here.
local function installed_components()
    local data = os.getenv("XDG_DATA_HOME")
    if not data or data == "" then data = os.getenv("HOME") .. "/.local/share" end
    local has = setmetatable({}, { __index = function() return true end })
    local f = io.open(data .. "/vela/components.toml", "r")
    if not f then return has end
    for line in f:lines() do
        local key, value = line:match("^%s*([%w_]+)%s*=%s*(%a+)")
        if key and (value == "true" or value == "false") then has[key] = value == "true" end
    end
    f:close()
    return has
end

local defaults = {
    -- Path of the vela binary; found automatically when nil.
    binary = nil,
    -- Toggle the launcher on a Super tap.
    launcher = true,
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
    -- Shortcut for the control center, e.g. "SUPER + T": a tap toggles it,
    -- holding it shows the panel only until the keys are released.
    panel_peek = nil,
    -- Held at least this long (ms), releasing closes the panel again.
    peek_ms = 280,
    -- Apply the Hyprland settings made in vela (Settings → Hyprland). They
    -- override hyprland.lua, so call setup() at its end.
    settings = true,
    -- Lua file with those settings; nil = $XDG_STATE_HOME/vela/hyprland.lua.
    -- The NixOS Home Manager module points it into your repository.
    settings_file = nil,
}

function M.setup(opts)
    local o = {}
    for k, v in pairs(defaults) do o[k] = v end
    for k, v in pairs(opts or {}) do o[k] = v end
    -- Options can switch parts off, but not on when they aren't installed.
    local has = installed_components()
    o.launcher = o.launcher and has.launcher
    o.shell = o.shell and has.panel
    o.idle = o.idle and has.idle
    o.settings = o.settings and has.hyprland
    if not has.panel then o.panel_peek = nil end
    local bin = o.binary or find_binary()
    local command = o.command or (bin .. " toggle")
    -- For vela's generated settings (Settings → Hyprland → Shortcuts): the
    -- launcher shortcut runs this, `vela_super_tap = false` turns the tap off,
    -- `vela_panel_keys` is the control center shortcut. They are loaded
    -- before the control center shortcut is bound.
    vela_launcher_command = command
    vela_super_tap = nil
    vela_panel_keys = nil

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
    if o.launcher then
        hl.on("input.keyboard.key", function(keycode, _, state)
            if vela_super_tap == false then return end
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
    end

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

    -- Control center: translucent panel, popups, workspace dots and the
    -- screen-share picker get blurred behind their visible parts; they
    -- animate themselves.
    hl.layer_rule({
        name         = "vela-shell",
        match        = { namespace = "^quickshell-(panel|notifications|osd|share)$" },
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

    if o.settings then
        local generated = o.settings_file
        if not generated then
            local state = os.getenv("XDG_STATE_HOME")
            if not state or state == "" then state = os.getenv("HOME") .. "/.local/state" end
            generated = state .. "/vela/hyprland.lua"
        end
        -- The daemon applies a rewritten file live only if it is this one.
        vela_settings_file = generated
        if exists(generated) then
            local ok, err = pcall(dofile, generated)
            if not ok then print("vela: " .. tostring(err)) end
        end
    end

    -- Settings → Shortcuts → Control center replaces setup()'s panel_peek.
    local peek_keys = has.panel and (vela_panel_keys or o.panel_peek) or nil
    if peek_keys then
        -- Compositor-side key events (they come before binds and reach us
        -- whatever has the focus, also while the panel has it).
        local last_key, peek_key, peeking, long, ptimer = nil, nil, false, false, nil
        hl.on("input.keyboard.key", function(keycode, _, state)
            if state == 1 then
                last_key = keycode
            elseif state == 0 and peeking and (keycode == peek_key or is_super[keycode]) then
                peeking = false
                if ptimer then ptimer:set_enabled(false) end
                ptimer = nil
                if long then hl.exec_cmd(bin .. " panel close") end
            end
        end)
        hl.bind(peek_keys, function()
            if peeking then return end
            peeking, long, peek_key = true, false, last_key
            hl.exec_cmd(bin .. " panel toggle")
            ptimer = hl.timer(function() long = true end, { timeout = o.peek_ms, type = "oneshot" })
        end, { description = "vela: control center (hold to peek)" })
    end

    return M
end

return M
