# System part of vela: Quick Share's access point helper (root through
# polkit) with hostapd and iw. Everything else is in the Home Manager module.
self:
{ config, lib, pkgs, ... }:
let
  cfg = config.programs.vela.hotspot;
in
{
  options.programs.vela.hotspot = {
    enable = lib.mkEnableOption "Quick Share over an access point of its own when the phone is not on your network";
    package = lib.mkOption {
      type = lib.types.package;
      default = self.packages.${pkgs.stdenv.hostPlatform.system}.default;
      description = "The vela package whose vela-hotspot-helper and polkit policy are installed.";
    };
  };

  config = lib.mkIf cfg.enable {
    environment.systemPackages = [ cfg.package pkgs.hostapd pkgs.iw ];
    security.polkit.enable = true;
  };
}
