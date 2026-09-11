{ grim
, slurp
, hyprpicker
, wl-clipboard
, jq
, patchelf
, file
, bash
, runCommand
}:
# Omarchy screenshot pipeline: grim + slurp + hyprpicker freeze + wl-copy + jq.
runCommand "oath-capture-pack"
  {
    nativeBuildInputs = [ patchelf file bash ];
    GRIM = grim;
    SLURP = slurp;
    HYPRPICKER = hyprpicker;
    WLCLIP = wl-clipboard;
    JQ = jq;
  } ''
  ${bash}/bin/bash ${./relocate-capture.sh} "$out"
''
