{
  lib,
  rustPlatform,
  pkg-config,
  wrapGAppsHook4,
  gtk4,
  libadwaita,
  gtk4-layer-shell,
  quickshell,
  hypridle,
  hyprsunset,
  brightnessctl,
  slurp,
  xdg-utils,
}:
let
  cargo = (builtins.fromTOML (builtins.readFile ../Cargo.toml)).package;
in
rustPlatform.buildRustPackage {
  pname = "vela";
  inherit (cargo) version;
  src = lib.cleanSource ../.;
  cargoLock.lockFile = ../Cargo.lock;

  nativeBuildInputs = [
    pkg-config
    wrapGAppsHook4
  ];
  buildInputs = [
    gtk4
    libadwaita
    gtk4-layer-shell
  ];

  # The tests need a Wayland compositor and Hyprland; CI runs them on Arch.
  doCheck = false;

  postInstall = ''
    for icon in data/icons/*.svg; do
      install -Dm644 "$icon" "$out/share/icons/hicolor/scalable/apps/$(basename "$icon")"
    done
    sed "s|^Exec=vela|Exec=$out/bin/vela|" data/vela.desktop \
      | install -Dm644 /dev/stdin "$out/share/applications/vela.desktop"
    install -Dm644 contrib/hyprland/vela.lua "$out/share/vela/vela.lua"
    (cd shell && find . -type f \( -name '*.qml' -o -name '*.svg' \) ! -name 'test-*' \
      -exec install -Dm644 {} "$out/share/vela/shell/{}" \;)
  '';

  # vela shell / vela idle start these by name.
  preFixup = ''
    gappsWrapperArgs+=(--prefix PATH : ${
      lib.makeBinPath [
        quickshell
        hypridle
        hyprsunset
        brightnessctl
        slurp
        xdg-utils
      ]
    })
  '';

  meta = {
    description = "Launcher, control center and every Hyprland setting";
    homepage = "https://github.com/Raindancer118/vela";
    license = lib.licenses.mit;
    mainProgram = "vela";
    platforms = lib.platforms.linux;
  };
}
