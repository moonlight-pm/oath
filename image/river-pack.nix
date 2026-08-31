{ lib
, river
, wlroots_0_20
, mesa
, xkeyboard_config
, libglvnd
, seatd
, libudev-zero
, libinput
, patchelf
, file
, bash
, runCommand
, riverSrc
, wlrootsSrc
}:
let
  wlroots = wlroots_0_20.overrideAttrs (old: {
    src = lib.cleanSource wlrootsSrc;
  });
  riverPkg = (river.override {
    wlroots_0_20 = wlroots;
    xwaylandSupport = false;
  }).overrideAttrs (_old: {
    src = lib.cleanSource riverSrc;
  });
in
runCommand "oath-river-pack"
  {
    nativeBuildInputs = [ patchelf file bash ];
    RIVER = riverPkg;
    MESA = mesa;
    XKB = xkeyboard_config;
    LIBGLVND = libglvnd;
    SEATD = seatd.bin or seatd;
    LIBUDEV_ZERO = libudev-zero;
    LIBINPUT_SHARE = "${libinput.out}/share/libinput";
  } ''
  ${bash}/bin/bash ${./relocate-river.sh} "$out"
''
