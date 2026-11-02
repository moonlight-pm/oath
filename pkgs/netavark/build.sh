# Output: bin/netavark, and components.json naming each crate in Cargo.lock,
# so advisories cover them.
export HOME=/tmp/home CARGO_HOME=/tmp/cargo PROTOC=protoc
mkdir -p "$HOME" /out/bin /out/share/doc/netavark
tar -xzf "$SRC/netavark-$VERSION.tar.gz"
cd "netavark-$VERSION"
tar -xzf "$SRC/netavark-v$VERSION-vendor.tar.gz"
mkdir -p .cargo
cat >.cargo/config.toml <<'EOT'
[source.crates-io]
replace-with = "vendored-sources"

[source.vendored-sources]
directory = "vendor"
EOT
cargo build --release --offline --locked --bin netavark
cp target/release/netavark /out/bin/
cp LICENSE /out/share/doc/netavark/
find vendor -maxdepth 2 -type f \( -iname 'LICEN[CS]E*' -o -iname 'COPYING*' -o -iname 'NOTICE*' \) \
	-exec cp --parents {} /out/share/doc/netavark/ \;
awk -v version="$VERSION" '
	BEGIN { printf "[\n  {\"name\": \"netavark\", \"version\": \"%s\", \"purl\": \"pkg:github/containers/netavark@v%s\"}", version, version }
	/^name = / { gsub(/"/, "", $3); n = $3 }
	/^version = / { gsub(/"/, "", $3); if (n != "netavark") printf ",\n  {\"name\": \"%s\", \"version\": \"%s\", \"purl\": \"pkg:cargo/%s@%s\"}", n, $3, n, $3 }
	END { print "\n]" }' Cargo.lock >/out/components.json

/out/bin/netavark version | grep -q "\"version\": \"$VERSION\""
