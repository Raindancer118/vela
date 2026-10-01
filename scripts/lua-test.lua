-- Tests contrib/hyprland/vela.lua against a fake `hl`: what setup()
-- registers for each component selection (components.toml).
--
--   lua scripts/lua-test.lua   (from the repository root)

local tmp = os.tmpname()
os.remove(tmp)
assert(os.execute("mkdir -p " .. tmp .. "/vela"))

local function run(manifest, opts)
    if manifest then
        local f = assert(io.open(tmp .. "/vela/components.toml", "w"))
        f:write(manifest)
        f:close()
    else
        os.remove(tmp .. "/vela/components.toml")
    end
    local seen = { on = {}, exec = {}, binds = {}, rules = {} }
    _G.hl = {
        on = function(event, fn)
            seen.on[event] = (seen.on[event] or 0) + 1
            if event == "input.keyboard.key" and not seen.key then seen.key = fn end
            if event == "hyprland.start" then fn() end
        end,
        exec_cmd = function(cmd) table.insert(seen.exec, cmd) end,
        bind = function(keys) table.insert(seen.binds, keys) end,
        unbind = function() end,
        dsp = { exec_cmd = function(cmd) return { exec = cmd } end },
        layer_rule = function(r) seen.rules[r.name] = true end,
        timer = function() return { set_enabled = function() end } end,
    }
    local saved = os.getenv
    os.getenv = function(k)
        if k == "XDG_DATA_HOME" then return tmp end
        if k == "XDG_STATE_HOME" then return tmp .. "/state" end
        return saved(k)
    end
    local vela = dofile("contrib/hyprland/vela.lua")
    vela.setup(opts)
    os.getenv = saved
    local function started(word)
        for _, c in ipairs(seen.exec) do
            if c:match(" " .. word .. "$") then return true end
        end
        return false
    end
    seen.shell, seen.idle = started("shell"), started("idle")
    -- The peek shortcut has a key handler of its own.
    seen.tap = (seen.on["input.keyboard.key"] or 0) - #seen.binds > 0
    return seen
end

local failures = 0
local function check(name, cond)
    if not cond then
        failures = failures + 1
        print("FAIL: " .. name)
    end
end

-- No file (package install): everything.
local s = run(nil, { panel_peek = "SUPER + T" })
check("full: Super tap", s.tap)
check("full: shell", s.shell)
check("full: idle", s.idle)
check("full: peek bind", #s.binds == 1)

-- Panel profile: no launcher tap, but shell and idle.
s = run('profile = "panel"\nlauncher = false\nclaude = false\nhyprland = false\npanel = true\nidle = true\nshare_picker = true\nupdates = true\n',
    { panel_peek = "SUPER + T" })
check("panel: no Super tap", not s.tap)
check("panel: shell", s.shell)
check("panel: idle", s.idle)
check("panel: peek bind", #s.binds == 1)

-- Minimal: launcher only.
s = run('profile = "minimal"\nlauncher = true\nclaude = false\nhyprland = false\npanel = false\nidle = false\nshare_picker = false\nupdates = false\n',
    { panel_peek = "SUPER + T" })
check("minimal: Super tap", s.tap)
check("minimal: no shell", not s.shell)
check("minimal: no idle", not s.idle)
check("minimal: no peek bind", #s.binds == 0)
check("minimal: daemon autostart", #s.exec == 1 and s.exec[1]:match("vela.service"))

-- Options can switch off, never switch on what isn't installed.
s = run('profile = "custom"\nlauncher = true\npanel = false\nidle = true\n', { shell = true, idle = false })
check("custom: shell stays off", not s.shell)
check("custom: idle switched off by option", not s.idle)

-- Comments and spacing.
s = run('# written by install.sh\nprofile="custom"\n  panel   =   false  # no\nidle=true\n', nil)
check("parsing: panel off", not s.shell)
check("parsing: idle on", s.idle)
check("parsing: launcher defaults to on", s.tap)

-- A Super tap toggles the launcher, unless Settings → Shortcuts switched the
-- tap off; the launcher shortcut from there runs the same command.
local function tap(seen)
    local before = #seen.exec
    seen.key(133, nil, 1)
    seen.key(133, nil, 0)
    return #seen.exec > before and seen.exec[#seen.exec]:match(" toggle$") ~= nil
end
s = run(nil, { binary = "/b/vela" })
check("tap: toggles by default", tap(s))
check("tap: command for the shortcut", vela_launcher_command == "/b/vela toggle")
local settings = tmp .. "/settings.lua"
local f = assert(io.open(settings, "w"))
f:write('vela_super_tap = false\n')
f:close()
s = run(nil, { binary = "/b/vela", settings_file = settings })
check("tap: switched off in the settings", not tap(s))
-- A reload without that line brings the tap back.
f = assert(io.open(settings, "w"))
f:write('-- nothing\n')
f:close()
s = run(nil, { binary = "/b/vela", settings_file = settings })
check("tap: back on after the setting is gone", tap(s))

-- The control center shortcut from Settings → Shortcuts wins over setup()'s
-- panel_peek, and works without it.
f = assert(io.open(settings, "w"))
f:write('vela_panel_keys = "SUPER + B"\n')
f:close()
s = run(nil, { settings_file = settings })
check("panel keys: bound from the settings", #s.binds == 1 and s.binds[1] == "SUPER + B")
s = run(nil, { settings_file = settings, panel_peek = "SUPER + T" })
check("panel keys: settings win over setup()", #s.binds == 1 and s.binds[1] == "SUPER + B")
s = run('profile = "minimal"\nlauncher = true\npanel = false\n', { settings_file = settings })
check("panel keys: nothing without the control center", #s.binds == 0)
f = assert(io.open(settings, "w"))
f:write('-- nothing\n')
f:close()
s = run(nil, { settings_file = settings, panel_peek = "SUPER + T" })
check("panel keys: setup() without a setting", #s.binds == 1 and s.binds[1] == "SUPER + T")

-- NixOS: the Home Manager module's call (store binary, settings in the
-- repository, panelPeek) with shortcuts set in Settings.
local repo = tmp .. "/nixos-repo-hyprland.lua"
f = assert(io.open(repo, "w"))
f:write('vela_super_tap = false\nvela_panel_keys = "SUPER + B"\n')
f:close()
s = run('# from Home Manager\nprofile = "custom"\nlauncher = true\npanel = true\nhyprland = true\n',
    { binary = "/nix/store/x-vela/bin/vela", settings_file = repo, panel_peek = "SUPER + T" })
check("nixos: Settings' control center keys", #s.binds == 1 and s.binds[1] == "SUPER + B")
check("nixos: Super tap off from Settings", not tap(s))
check("nixos: launcher command uses the store binary", vela_launcher_command == "/nix/store/x-vela/bin/vela toggle")

-- NixOS: the selection saved in the state directory wins over Home
-- Manager's default; until it exists the default counts.
local chosen = tmp .. "/nixos-repo-components.toml"
local hm = '# from Home Manager\nprofile = "custom"\nlauncher = true\npanel = true\nidle = true\nhyprland = true\n'
s = run(hm, { components_file = chosen })
check("nixos: Home Manager default before Settings saved", s.shell and s.idle)
f = assert(io.open(chosen, "w"))
f:write('# Written by vela\nprofile = "custom"\nlauncher = true\npanel = false\nidle = false\nhyprland = true\n')
f:close()
s = run(hm, { components_file = chosen })
check("nixos: Settings' selection wins", not s.shell and not s.idle and tap(s))
os.remove(chosen)

os.execute("rm -rf " .. tmp)
if failures > 0 then
    os.exit(1)
end
print("vela.lua: all checks passed")
