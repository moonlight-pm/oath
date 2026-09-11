{ hyprland
, mesa
, xkeyboard_config
, libglvnd
, libudev-zero
, libinput
, patchelf
, file
, bash
, runCommand
}:
let
  hypr = hyprland.override {
    withSystemd = false;
    wrapRuntimeDeps = false;
    enableXWayland = true;
  };
in
runCommand "oath-hyprland-pack"
  {
    nativeBuildInputs = [ patchelf file bash ];
    HYPRLAND = hypr;
    MESA = mesa;
    XKB = xkeyboard_config;
    LIBGLVND = libglvnd;
    LIBUDEV_ZERO = libudev-zero;
    LIBINPUT_SHARE = "${libinput.out}/share/libinput";
  } ''
  ${bash}/bin/bash ${./relocate-hyprland.sh} "$out"
''
