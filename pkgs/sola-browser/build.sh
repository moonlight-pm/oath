# sola-browser, built offline against pkg:sola-crates and pkg:cef. The
# build finds CEF in $HOME/.cache (cef-dll-sys) and $XDG_CACHE_HOME
# (Sola's), and bakes the latter into RUNPATH, so it names pkg:cef's
# slot. RUSTFLAGS replaces the tree's NixOS rpaths.
export CARGO_HOME=/tmp/cargo HOME=/tmp/home XDG_CACHE_HOME=/oath/store/pkg/cef/live
export RUSTFLAGS="-C link-arg=-Wl,--enable-new-dtags"
mkdir -p "$CARGO_HOME" "$HOME/.cache/sola"
cp /oath/store/pkg/sola-crates/live/vendor.toml "$CARGO_HOME/config.toml"
v=$(cat "$SRC/sola/cef-version")
ln -s /oath/store/pkg/cef/live/cef "$HOME/.cache/sola/cef-$v"
cp -R "$SRC/sola" /tmp/sola
cd /tmp/sola
cargo build --release --frozen -p sola-browser
mkdir -p /out/bin
cp target/release/sola-browser /out/bin/
ln -s /oath/store/pkg/cef/live/cef /out/cef
