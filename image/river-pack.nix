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
  # Pitcairn nest: radeonsi samples INVALID-modifier dmabufs using GEM
  # tiling, but rejected width*bpp pitches that are not macrotile-aligned.
  mesaSi = mesa.overrideAttrs (old: {
    patches = (old.patches or []) ++ [
      ./mesa-patches/0001-ac-surface-si-imported-implicit-pitch.patch
      ./mesa-patches/0002-radv-si-linear-export.patch
    ];
  });
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
    MESA = mesaSi;
    XKB = xkeyboard_config;
    LIBGLVND = libglvnd;
    SEATD = seatd.bin or seatd;
    LIBUDEV_ZERO = libudev-zero;
    LIBINPUT_SHARE = "${libinput.out}/share/libinput";
  } ''
  ${bash}/bin/bash ${./relocate-river.sh} "$out"
''
