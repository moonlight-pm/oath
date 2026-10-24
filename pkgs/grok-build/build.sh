# The vendor's binary in libexec/, and bin/grok, which keeps Grok from
# updating itself: the store holds the version, and a newer one is a new
# pin in pkgs/grok-build. Grok's updater has no switch but its config's
# `[cli] auto_update`, so bin/grok sets it to false before Grok starts.
export HOME=/tmp/home
mkdir -p "$HOME" /out/bin /out/libexec/grok-build
b=/out/libexec/grok-build/grok
zstd -dc "$SRC/grok-$VERSION-linux-x86_64.zst" >"$b"
chmod 755 "$b"
cat >/out/bin/grok <<'EOT'
#!/bin/sh
# Grok Build @VERSION@, built here from its recipe (R31). Updates are a new
# pin in Oath's pkgs/grok-build, so Grok's own updater is off.
c=${GROK_HOME:-$HOME/.grok}/config.toml
if ! grep -qsx 'auto_update = false' "$c"; then
	if grep -qs '^auto_update *=' "$c"; then
		sed -i --follow-symlinks 's/^auto_update *=.*/auto_update = false/' "$c"
	elif grep -qs '^\[cli\]' "$c"; then
		sed -i --follow-symlinks '/^\[cli\]/a auto_update = false' "$c"
	else
		mkdir -p "${c%/*}" && { [ ! -s "$c" ] || echo; printf '[cli]\nauto_update = false\n'; } >>"$c"
	fi
fi
exec @LIVE@/libexec/grok-build/grok "$@"
EOT
sed -i "s|@VERSION@|$VERSION|; s|@LIVE@|$LIVE|" /out/bin/grok
chmod 755 /out/bin/grok
"$b" --version | grep -q "^grok $VERSION "
# The config edit, for no config, a [cli] without the key, the key set
# true under [cli] among other tables, and a config with no [cli].
sed 's|^exec .*|exit 0|' /out/bin/grok >/tmp/w
t=$HOME/.grok/config.toml
sh /tmp/w
[ "$(cat "$t")" = "$(printf '[cli]\nauto_update = false')" ]
printf '[cli]\ninstaller = "internal"\n' >"$t"
sh /tmp/w
[ "$(sed -n 2p "$t")" = "auto_update = false" ]
printf '[ui]\ntheme = "dark"\n\n[cli]\nauto_update = true\n' >"$t"
sh /tmp/w
[ "$(grep -c auto_update "$t")" = 1 ] && grep -qx 'auto_update = false' "$t"
printf 'model = "grok-4"' >"$t"
sh /tmp/w
[ "$(cat "$t")" = "$(printf 'model = "grok-4"\n[cli]\nauto_update = false')" ]
