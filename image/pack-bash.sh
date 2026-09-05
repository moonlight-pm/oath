#!/bin/sh
# Pack pkg:bash from a static musl GNU bash ELF (borrowed, like pkg:grok).
# Guest /bin/bash must be the ELF — a #!/bin/sh wrapper loses Grok's
# `bash -O extglob` and `builtin`.
set -eu

out=${1:?out}
here=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
fetch=$here/fetch-url.sh
cache=${OATH_FETCH:-${TMPDIR:-/tmp}/oath-fetch}

BASH_VER=5.2.15
# robxu9/bash-static: GNU bash + musl, single ELF. No GNU make on canto.
BASH_URL=https://github.com/robxu9/bash-static/releases/download/5.2.015-1.2.3-2/bash-linux-x86_64

if [ -e "$out" ]; then
	chmod -R u+w "$out" 2>/dev/null || true
	rm -rf "$out"
fi
mkdir -p "$out/bin" "$out/libexec"

tarball=$cache/bash-linux-x86_64
mkdir -p "$cache"
if [ -n "${OATH_BASH_ELF:-}" ] && [ -f "$OATH_BASH_ELF" ]; then
	cp -a "$OATH_BASH_ELF" "$out/libexec/bash"
else
	sh "$fetch" "$BASH_URL" "$tarball"
	cp -a "$tarball" "$out/libexec/bash"
fi
chmod 755 "$out/libexec/bash"
# Farm link is the ELF itself (not a sh wrapper).
cp -a "$out/libexec/bash" "$out/bin/bash"
chmod 755 "$out/bin/bash"

printf '%s\n' "bash $BASH_VER static-musl" >"$out/REV"
cat >"$out/INDEX.md" <<'EOF'
# pkg:bash

GNU bash as a borrowed static musl ELF. Steam's launcher scripts and
Grok's agent shell need real bash (`builtin`, `shopt`, `-O extglob`).
Busybox ash is not bash. Removable.
EOF

chmod -R u+rwX "$out"
echo "packed bash -> $out"
