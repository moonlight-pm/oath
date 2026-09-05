#!/bin/sh
# Fetch a static musl GNU bash and install as /bin/bash + pkg:bash store.
# Grok's tool requires real bash (builtin, shopt, -O extglob). Canto has
# no GNU make, so we borrow a static ELF (same class as pkg:grok).
set -eu

url='https://github.com/robxu9/bash-static/releases/download/5.2.015-1.2.3-2/bash-linux-x86_64'
tmp=${OATH_BASH_TMP:-/tmp/bash-linux-x86_64}
log=${OATH_BASH_LOG:-/tmp/oath-fetch-bash.log}
store=/oath/store/pkg/bash

{
	echo "==> fetch $url"
	if command -v curl >/dev/null 2>&1; then
		curl -fL --retry 3 --retry-delay 2 --max-time 60 -o "$tmp" "$url"
	else
		wget -O "$tmp" "$url"
	fi
	chmod 755 "$tmp"
	# Sanity: ELF, not an HTML error page.
	if ! head -c 4 "$tmp" | grep -q "ELF"; then
		echo "not an ELF: $tmp" >&2
		head -c 200 "$tmp" >&2 || true
		exit 1
	fi
	"$tmp" --version | head -n1
	if [ "$(id -u)" = 0 ]; then
		as_root() { "$@"; }
	else
		as_root() { sudo -n "$@"; }
	fi
	as_root mkdir -p "$store/bin" "$store/libexec"
	as_root cp "$tmp" "$store/libexec/bash"
	as_root chmod 755 "$store/libexec/bash"
	printf '%s\n' '#!/bin/sh' 'exec /oath/store/pkg/bash/libexec/bash "$@"' | as_root tee "$store/bin/bash" >/dev/null
	as_root chmod 755 "$store/bin/bash"
	# Live /bin/bash must be the ELF (Grok execs it directly; a wrapper
	# that execs ash loses bash-only flags). Store wrapper is for PATH farm.
	as_root cp "$tmp" /bin/bash
	as_root chmod 755 /bin/bash
	echo "==> /bin/bash ok"
	/bin/bash --version | head -n1
} >>"$log" 2>&1
echo ok >>"$log"
