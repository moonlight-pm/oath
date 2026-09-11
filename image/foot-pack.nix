{ foot
, xkeyboard_config
, libudev-zero
, libinput
, patchelf
, file
, bash
, runCommand
}:
runCommand "oath-foot-pack"
  {
    nativeBuildInputs = [ patchelf file bash ];
    FOOT = foot;
    FOOT_TERMINFO = foot.terminfo;
    XKB = xkeyboard_config;
    LIBUDEV_ZERO = libudev-zero;
    LIBINPUT_SHARE = "${libinput.out}/share/libinput";
  } ''
  ${bash}/bin/bash ${./relocate-foot.sh} "$out"
''
