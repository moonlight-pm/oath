{ wayland
, libxkbcommon
, fontconfig
, freetype
, inter
, libffi
, libglvnd
, vulkan-loader
, runCommand
}:
# Host `cargo build` produces the Sola ELFs. This tree is the dlopen
# runtime relocate-sola.sh walks (wayland, fonts, vulkan loader).
runCommand "oath-sola-rt" { } ''
  mkdir -p $out
  ln -s ${wayland} $out/wayland
  ln -s ${libxkbcommon} $out/xkbcommon
  ln -s ${fontconfig.lib or fontconfig} $out/fontconfig
  ln -s ${freetype} $out/freetype
  ln -s ${inter} $out/inter
  ln -s ${libffi} $out/libffi
  ln -s ${libglvnd} $out/libglvnd
  ln -s ${vulkan-loader} $out/vulkan-loader
''
