# pkg:oath: the Oath binaries, built by pkg:rust. $SRC/crates holds every
# crates.io crate in Cargo.lock, pinned by its checksum; cargo reads them
# as a vendored source and never touches the network.
export CARGO_HOME=/tmp/cargo
mkdir -p "$CARGO_HOME" /tmp/vendor
for c in "$SRC"/crates/*.crate; do
	tar -xzf "$c" -C /tmp/vendor
	n=${c##*/}
	sum=$(sha256sum "$c")
	printf '{"files":{},"package":"%s"}\n' "${sum%% *}" >"/tmp/vendor/${n%.crate}/.cargo-checksum.json"
done
cat >"$CARGO_HOME/config.toml" <<'EOF'
[source.crates-io]
replace-with = "vendored"

[source.vendored]
directory = "/tmp/vendor"
EOF
cp -R "$FILES/tree" /tmp/oath
cd /tmp/oath
musl="--frozen --target x86_64-unknown-linux-musl -p oath -p oath-init"
efi="--frozen --target x86_64-unknown-uefi -p oath-efi --features uefi-app"
cargo build --release $musl
cargo build --release $efi
r=target/x86_64-unknown-linux-musl/release
mkdir -p /out/bin /out/lib/oath /out/boot
cp $r/oath /out/bin/oath
cp $r/oath-init /out/lib/oath/init
cp $r/serial-login $r/sudo /out/lib/oath/
cp target/x86_64-unknown-uefi/release/oath-efi.efi /out/boot/BOOTX64.EFI

# components.json: oath, and the crates.io crates its binaries build from,
# sorted by purl.
cargo tree $musl -e normal --prefix none -f '{p}' >/tmp/tree
cargo tree $efi -e normal --prefix none -f '{p}' >>/tmp/tree
awk 'NF >= 2 && !/ \(\// { v = $2; sub(/^v/, "", v); print "pkg:cargo/" $1 "@" v, $1, v }' /tmp/tree |
	LC_ALL=C sort -u >/tmp/crates
{
	printf '{\n  "components": [\n'
	while read -r purl n v; do
		printf '    {\n      "name": "%s",\n      "purl": "%s",\n      "version": "%s"\n    },\n' \
			"$n" "$purl" "$v"
	done </tmp/crates
	printf '    {\n      "name": "oath",\n      "purl": "pkg:generic/oath@%s",\n      "version": "%s"\n    }\n' \
		"$VERSION" "$VERSION"
	printf '  ]\n}\n'
} >/out/components.json
