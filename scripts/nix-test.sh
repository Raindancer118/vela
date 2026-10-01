#!/usr/bin/env bash
# NixOS side: builds the flake's package and evaluates the Home Manager
# module in a throwaway home configuration, then checks what it generates.
# Needs nix with flakes; run from anywhere.
set -euo pipefail
repo="$(cd "$(dirname "$0")/.." && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
nix() { command nix --extra-experimental-features 'nix-command flakes' "$@"; }

nix flake check --no-build "path:$repo"
out="$(nix build --no-link --print-out-paths "path:$repo#default")"
for f in bin/vela bin/vela-daemon bin/vela-share-picker share/vela/vela.lua share/vela/shell/Config.qml share/applications/vela.desktop; do
    [[ -e "$out/$f" ]] || { echo "FAIL: package lacks $f"; exit 1; }
done
"$out/bin/vela" --version
# Shortcuts from Settings reach Hyprland through these globals.
for g in vela_super_tap vela_panel_keys vela_launcher_command vela_settings_file; do
    grep -q "$g" "$out/share/vela/vela.lua" || { echo "FAIL: packaged vela.lua lacks $g"; exit 1; }
done

cat >"$tmp/flake.nix" <<NIX
{
  inputs.vela.url = "path:$repo";
  inputs.nixpkgs.follows = "vela/nixpkgs";
  inputs.home-manager = {
    url = "github:nix-community/home-manager";
    inputs.nixpkgs.follows = "nixpkgs";
  };
  outputs = { nixpkgs, home-manager, vela, ... }:
    let
      home = modules: home-manager.lib.homeManagerConfiguration {
        pkgs = nixpkgs.legacyPackages.x86_64-linux;
        modules = [
          vela.homeManagerModules.default
          {
            home = { username = "u"; homeDirectory = "/home/u"; stateVersion = "25.05"; };
            wayland.windowManager.hyprland = { enable = true; configType = "lua"; };
            programs.vela.enable = true;
          }
        ] ++ modules;
      };
    in {
      default = home [ ];
      custom = home [{
        programs.vela.components = { claude = false; share_picker = true; };
        programs.vela.nixos.stateDir = "/home/u/nixos/vela";
      }];
      typo = home [{ programs.vela.components.lancher = false; }];
    };
}
NIX
file() { nix eval --raw "path:$tmp#$1.config.xdg.$2.\"$3\".text"; }
fails=0
expect() { grep -qx -- "$2" <<<"$3" || { echo "FAIL: $1: expected line '$2' in:"; echo "$3"; fails=$((fails + 1)); }; }
no() { ! grep -qx -- "$2" <<<"$3" || { echo "FAIL: $1: unexpected line '$2'"; fails=$((fails + 1)); }; }

c="$(file default dataFile vela/components.toml)"
expect "default: profile" 'profile = "custom"' "$c"
expect "default: launcher" 'launcher = true' "$c"
expect "default: no pacman updates" 'updates = false' "$c"
expect "default: no share picker" 'share_picker = false' "$c"
[[ "$(nix eval "path:$tmp#default.config.xdg.configFile" --apply 'f: f ? "hypr/xdph.conf"')" == false ]] ||
    { echo "FAIL: default: xdph.conf written"; fails=$((fails + 1)); }

c="$(file custom dataFile vela/components.toml)"
expect "custom: claude off" 'claude = false' "$c"
expect "custom: share picker" 'share_picker = true' "$c"
grep -q 'custom_picker_binary = /nix/store/.*/bin/vela-share-picker' <<<"$(file custom configFile hypr/xdph.conf)" ||
    { echo "FAIL: custom: xdph.conf"; fails=$((fails + 1)); }
expect "custom: pointer" 'state_dir = "/home/u/nixos/vela"' "$(file custom configFile vela/nixos.toml)"
grep -q 'settings_file.*= "/home/u/nixos/vela/hyprland.lua"' \
    <<<"$(nix eval --raw "path:$tmp#custom.config.wayland.windowManager.hyprland.extraConfig")" ||
    { echo "FAIL: custom: settings_file in setup()"; fails=$((fails + 1)); }

if nix eval --raw "path:$tmp#typo.activationPackage.drvPath" >"$tmp/typo.log" 2>&1; then
    echo "FAIL: typo: unknown component accepted"
    fails=$((fails + 1))
elif ! grep -q "unknown lancher" "$tmp/typo.log"; then
    echo "FAIL: typo: unclear error"; cat "$tmp/typo.log"; fails=$((fails + 1))
fi
nix eval --raw "path:$tmp#default.activationPackage.drvPath" >/dev/null

((fails == 0)) || exit 1
echo "nix: all checks passed"
