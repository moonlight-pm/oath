#!/bin/sh
# Fetch a URL to a dest file. Optional SHA256 as $3.
# Busybox ash. Uses curl, falls back to wget.
set -eu

url=${1:?url}
dest=${2:?dest}
want=${3:-}

mkdir -p "$(dirname "$dest")"
if [ -f "$dest" ] && [ -n "$want" ]; then
	have=$(sha256sum "$dest" | awk '{print $1}')
	if [ "$have" = "$want" ]; then
		echo "cached $dest"
		exit 0
	fi
	rm -f "$dest"
fi

echo "fetch $url"
if command -v curl >/dev/null 2>&1; then
	curl -fL --retry 3 --retry-delay 2 -o "$dest" "$url"
else
	wget -O "$dest" "$url"
fi

if [ -n "$want" ]; then
	have=$(sha256sum "$dest" | awk '{print $1}')
	if [ "$have" != "$want" ]; then
		echo "sha256 mismatch $dest" >&2
		echo "  want $want" >&2
		echo "  have $have" >&2
		exit 1
	fi
fi
