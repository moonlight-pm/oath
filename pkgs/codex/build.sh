# The vendor's package in libexec/codex/ as it ships (codex finds its
# resources beside bin/ through codex-package.json), the voice helper's
# loader pointed at Oath's, and bin/codex, which turns Codex's update
# check off: the store holds the version, and a newer one is a new pin in
# pkgs/codex.
export HOME=/tmp/home
p=/out/libexec/codex
mkdir -p "$HOME" /out/bin "$p"
zstd -dc "$SRC/codex-package-x86_64-unknown-linux-musl.tar.zst" | tar -x --no-same-owner -C "$p"
grep -q "\"version\": \"$VERSION\"" "$p/codex-package.json"
# The rest is static; the voice helper's .interp, at the start of the
# file, names /lib64/ld-linux-x86-64.so.2 and Oath's loader is /lib's
# (R17). As for Claude Code, the shorter name is written over the old one,
# NUL-padded, and nothing in the file moves.
v=$p/codex-resources/voice/bin/codex-voice-host
off=$(grep -obUaF /lib64/ld-linux-x86-64.so.2 "$v" | head -n 1 | cut -d: -f1)
[ "$off" -lt 4096 ]
printf '/lib/ld-linux-x86-64.so.2\0\0\0' | dd of="$v" bs=1 seek="$off" conv=notrunc status=none
/lib/ld-linux-x86-64.so.2 --list "$v" >/dev/null
cat >/out/bin/codex <<EOT
#!/bin/sh
# Codex $VERSION, built here from its recipe (R31). Updates are a new pin
# in Oath's pkgs/codex, so Codex's own update check is off.
exec $LIVE/libexec/codex/bin/codex -c check_for_update_on_startup=false "\$@"
EOT
chmod 755 /out/bin/codex
"$p/bin/codex" -c check_for_update_on_startup=false --version 2>/dev/null | grep -qx "codex-cli $VERSION"
