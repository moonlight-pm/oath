# The Sola binaries, built offline against pkg:sola-crates, and share/.
export CARGO_HOME=/tmp/cargo HOME=/tmp/home
mkdir -p "$CARGO_HOME" "$HOME"
cp /oath/store/pkg/sola-crates/live/vendor.toml "$CARGO_HOME/config.toml"
cp -R "$SRC/sola" /tmp/sola
cd /tmp/sola
bins="sola-bus sola-call sola-river sola-shell sola-session sola-terminal solactl sola-oath"
cargo build --release --frozen $(for b in $bins; do printf -- '-p %s ' "$b"; done)
mkdir -p /out/bin
for b in $bins; do
	cp "target/release/$b" /out/bin/
done
share=/out/share
mkdir -p "$share/icons" "$share/cursors"
cp -R crates/sola-assets/icons/sola "$share/icons/sola"
for a in "$SRC"/lucide-*.tar.gz "$SRC"/simple-icons-*.tar.gz "$SRC"/McMojave-*.tar.gz; do
	tar -xzf "$a" -C /tmp
done
mkdir -p "$share/icons/lucide" "$share/icons/simpleicons" "$share/cursors/McMojave"
cp /tmp/lucide-*/icons/*.svg "$share/icons/lucide/"
cp /tmp/simple-icons-*/icons/*.svg "$share/icons/simpleicons/"
cp -R /tmp/McMojave-cursors-*/dist/. "$share/cursors/McMojave/"
