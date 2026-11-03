# Output: bin/writ, static like Oath's other binaries, and components.json naming itself.
# Its box needs a user namespace, which the sandbox blocks: `writ check` tests that on a machine.
export HOME=/tmp/home CARGO_HOME=/tmp/cargo RUSTFLAGS="-C target-feature=+crt-static"
target=x86_64-unknown-linux-gnu
mkdir -p "$HOME" /out/bin
cp -a "$SRC/writ" /tmp/writ
cd /tmp/writ
cargo test --frozen --target $target
cargo build --release --frozen --target $target
cp target/$target/release/writ /out/bin/writ
if readelf -lW /out/bin/writ | grep -q INTERP; then
	echo "/out/bin/writ is dynamically linked; Oath's binaries are static" >&2
	exit 1
fi
cat >/out/components.json <<JSON
{
  "components": [
    {
      "name": "writ",
      "purl": "pkg:generic/writ@$VERSION",
      "version": "$VERSION"
    }
  ]
}
JSON
/out/bin/writ --help | grep -q 'writ check'
