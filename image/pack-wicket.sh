#!/bin/sh
# Assemble build/wicket-pack for `cargo make install --wicket`.
# The desk image does not contain these bits. The installer copies this
# directory onto the new root when it is present.
set -e
ROOT=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
OUT="$ROOT/build/wicket-pack"
BIN="$OUT/bin"
mkdir -p "$BIN"

wicket_src=${WICKET_SRC:-$ROOT/../Wicket}
musl="$wicket_src/target/x86_64-unknown-linux-musl/release"
if [ -d "$musl" ]; then
    for name in wicketd wicket-idp wicket-admin wicket-setup; do
        if [ -x "$musl/$name" ]; then
            cp -f "$musl/$name" "$BIN/$name"
            chmod 755 "$BIN/$name"
        fi
    done
fi

if [ -n "${CADDY:-}" ] && [ -x "$CADDY" ]; then
    cp -f "$CADDY" "$BIN/caddy"
    chmod 755 "$BIN/caddy"
elif [ ! -x "$BIN/caddy" ]; then
    tmp=$(mktemp)
    url=${CADDY_URL:-https://github.com/caddyserver/caddy/releases/download/v2.10.2/caddy_2.10.2_linux_amd64.tar.gz}
    if curl -fsSL "$url" -o "$tmp"; then
        tar -xzf "$tmp" -C "$BIN" caddy
        chmod 755 "$BIN/caddy"
    fi
    rm -f "$tmp"
fi

if [ -n "${POSTGRES_PREFIX:-}" ] && [ -d "$POSTGRES_PREFIX/bin" ]; then
    mkdir -p "$OUT/postgresql"
    cp -a "$POSTGRES_PREFIX/." "$OUT/postgresql/"
fi

echo "wicket pack: $OUT"
ls -l "$BIN" 2>/dev/null || true
