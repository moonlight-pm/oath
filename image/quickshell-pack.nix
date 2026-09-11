{ quickshell
, qt6
, mesa
, libglvnd
, libudev-zero
, libinput
, xkeyboard_config
, patchelf
, file
, bash
, perl
, runCommand
}:
runCommand "oath-quickshell-pack"
  {
    nativeBuildInputs = [ patchelf file bash perl ];
    QUICKSHELL = quickshell;
    QTBASE = qt6.qtbase;
    QTDECLARATIVE = qt6.qtdeclarative;
    QTWAYLAND = qt6.qtwayland;
    QTSVG = qt6.qtsvg;
    MESA = mesa;
    LIBGLVND = libglvnd;
    LIBUDEV_ZERO = libudev-zero;
    LIBINPUT_SHARE = "${libinput.out}/share/libinput";
    XKB = xkeyboard_config;
  } ''
  ${bash}/bin/bash ${./relocate-quickshell.sh} "$out"
''
