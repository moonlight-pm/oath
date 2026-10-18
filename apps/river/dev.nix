# Build host only (novus): a shell to build and test the C port of River
# against the nixpkgs Sola pins (wlroots 0.20). Not part of any Oath image.
let
  pkgs = import (builtins.fetchTarball {
    url = "https://github.com/NixOS/nixpkgs/archive/d233902339c02a9c334e7e593de68855ad26c4cb.tar.gz";
    sha256 = "sha256-30sZNZoA1cqF5JNO9fVX+wgiQYjB7HJqqJ4ztCDeBZE=";
  }) { };
in
pkgs.mkShell {
  nativeBuildInputs = with pkgs; [ pkg-config wayland-scanner gnumake gcc ];
  buildInputs = with pkgs; [
    wlroots_0_20 wayland wayland-protocols libxkbcommon pixman libinput libevdev
    libxcb libxcb-wm xwayland
  ];
  packages = with pkgs; [ wayland-utils foot grim wlr-randr gdb valgrind xterm swaybg imagemagick python3 ];
}
