self:
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.programs.vela;
  inherit (lib) mkOption mkEnableOption mkIf types;
  setupArgs = {
    binary = lib.getExe cfg.package;
  }
  // lib.optionalAttrs (cfg.nixos.stateDir != null) { settings_file = "${cfg.nixos.stateDir}/hyprland.lua"; }
  // lib.optionalAttrs (cfg.panelPeek != null) { panel_peek = cfg.panelPeek; };
in
{
  options.programs.vela = {
    enable = mkEnableOption "vela, launcher and control center for Hyprland";

    package = mkOption {
      type = types.package;
      default = self.packages.${pkgs.stdenv.hostPlatform.system}.default;
      defaultText = lib.literalExpression "vela.packages.\${system}.default";
      description = "The vela package.";
    };

    panelPeek = mkOption {
      type = types.nullOr types.str;
      default = null;
      example = "SUPER + B";
      description = "Shortcut for the control center: a tap toggles it, holding it peeks.";
    };

    nixos = {
      stateDir = mkOption {
        type = types.nullOr (types.strMatching "/.*");
        default = null;
        example = "/home/me/nixos/home/me/vela";
        description = ''
          Absolute path of a directory in your NixOS repository. vela then keeps
          config.toml and its Hyprland settings there instead of ~/.config/vela
          and ~/.local/state/vela, and Settings offers "Apply & rebuild" while
          git reports uncommitted changes in it. A string, not a Nix path:
          vela writes to it.
        '';
      };

      rebuildCommand = mkOption {
        type = types.str;
        default = "sudo nixos-rebuild switch --flake .";
        example = "rebuild";
        description = ''
          Run in a terminal, in stateDir, by "Apply & rebuild". Split into
          words like a shell would, but not run through one.
        '';
      };
    };
  };

  config = mkIf cfg.enable {
    assertions = [
      {
        assertion = config.wayland.windowManager.hyprland.configType == "lua";
        message = ''programs.vela needs wayland.windowManager.hyprland.configType = "lua" (Hyprland 0.55+).'';
      }
    ];

    # The fonts below are only found with fontconfig on.
    fonts.fontconfig.enable = lib.mkDefault true;

    home.packages = [
      cfg.package
    ]
    ++ (with pkgs; [
      # Control center font and icons.
      material-symbols
      rubik
      papirus-icon-theme
    ]);

    # Same unit as contrib/systemd/vela.service.in; vela.lua (re)starts it.
    systemd.user.services.vela = {
      Unit = {
        Description = "vela application launcher";
        Documentation = "https://github.com/Raindancer118/vela";
        PartOf = [ "graphical-session.target" ];
        After = [ "graphical-session.target" ];
      };
      Service = {
        Type = "simple";
        ExecStart = "${cfg.package}/bin/vela-daemon";
        Restart = "on-abnormal";
        RestartSec = 1;
        KillMode = "process";
        Environment = [ "RUST_LOG=vela=info" ];
      };
    };

    xdg.configFile."vela/nixos.toml" = mkIf (cfg.nixos.stateDir != null) {
      text = ''
        state_dir = ${builtins.toJSON cfg.nixos.stateDir}
        rebuild = ${builtins.toJSON cfg.nixos.rebuildCommand}
      '';
    };

    # Appended at the end of hyprland.lua, so vela's settings win.
    wayland.windowManager.hyprland.extraConfig = ''
      -- vela: Super tap, control center, idle and vela's Hyprland settings
      dofile("${cfg.package}/share/vela/vela.lua").setup(${lib.generators.toLua { } setupArgs})
    '';
  };
}
