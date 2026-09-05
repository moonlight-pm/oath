#!/bin/sh
# Pack pkg:pkg-config. Empty .pc farm: --exists fails so crates that
# probe openssl/libcrypto fall back to bundled sources (aws-lc-sys).
set -eu

out=${1:?out}

if [ -e "$out" ]; then
	chmod -R u+w "$out" 2>/dev/null || true
	rm -rf "$out"
fi
mkdir -p "$out/bin"

cat >"$out/bin/pkg-config" <<'EOF'
#!/bin/sh
# Oath pkg-config: no .pc database yet.
case "${1:-}" in
--version|-v)
	echo "2.0.0"
	exit 0
	;;
--help|-h)
	echo "pkg-config (Oath empty farm)"
	exit 0
	;;
--list-all)
	exit 0
	;;
--exists|--atleast-pkgconfig-version|--atleast-version|--exact-version|--max-version)
	exit 1
	;;
esac
exit 1
EOF
chmod 755 "$out/bin/pkg-config"
ln -s pkg-config "$out/bin/pkgconf"

printf '%s\n' "pkg-config empty-farm 2.0.0" >"$out/REV"

cat >"$out/INDEX.md" <<'EOF'
# pkg:pkg-config

`/bin/pkg-config` exists so build scripts do not ENOENT. There is no
`.pc` farm yet; `--exists` fails and bundled C (aws-lc-sys) compiles
with `pkg:cc` + `pkg:cmake`. Removable.
EOF

chmod -R u+rwX "$out"
echo "packed pkg-config -> $out"
