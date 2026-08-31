{ wayland
, libxkbcommon
, fontconfig
, freetype
, inter
, libffi
, libglvnd
, vulkan-loader
, tmux
, ncurses
, glibcLocales
, runCommand
}:
# Host `cargo build` produces the Sola ELFs. This tree is the dlopen
# runtime relocate-sola.sh walks (wayland, fonts, vulkan loader, tmux).
let
  cUtf8 = glibcLocales.override {
    allLocales = false;
    locales = [ "C.UTF-8/UTF-8" ];
  };
in
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
  ln -s ${tmux} $out/tmux
  ln -s ${ncurses} $out/ncurses
  ln -s ${cUtf8} $out/locales
''
