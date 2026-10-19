# The vendor's binary in libexec/, its loader pointed at Oath's, and
# bin/claude, which keeps it from updating itself: the store holds the
# version, and a newer one is a new pin in pkgs/claude-code.
export HOME=/tmp/home
mkdir -p "$HOME" /out/bin /out/libexec/claude-code
b=/out/libexec/claude-code/claude
cp "$SRC/claude" "$b"
# The ELF's .interp, at the start of the file, names
# /lib64/ld-linux-x86-64.so.2; Oath's loader is /lib's (R17). The new name
# is shorter, so it is written over the old one, NUL-padded, and nothing in
# the file moves (Bun finds its bundle, an ELF section, by address).
off=$(grep -obUaF /lib64/ld-linux-x86-64.so.2 "$b" | head -n 1 | cut -d: -f1)
[ "$off" -lt 4096 ]
printf '/lib/ld-linux-x86-64.so.2\0\0\0' | dd of="$b" bs=1 seek="$off" conv=notrunc status=none
chmod 755 "$b"
cat > /out/bin/claude <<EOF
#!/bin/sh
# Claude Code $VERSION, built here from its recipe (R23). Updates are a new
# pin in Oath's pkgs/claude-code, not Claude Code's own updater.
DISABLE_UPDATES=1 exec $LIVE/libexec/claude-code/claude "\$@"
EOF
chmod 755 /out/bin/claude
"$b" --version | grep -qx "$VERSION (Claude Code)"
