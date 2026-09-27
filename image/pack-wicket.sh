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

# Postgres is a glibc Nix closure. The binaries keep their /nix/store
# interpreter paths, so the installer also copies nix-store/ onto /nix/store.
if [ -z "${POSTGRES_PREFIX:-}" ]; then
    POSTGRES_PREFIX=$(find /nix/store -maxdepth 1 -type d -name '*-postgresql-and-plugins-*' 2>/dev/null | sort | tail -1 || true)
fi
if [ -n "${POSTGRES_PREFIX:-}" ] && [ -d "$POSTGRES_PREFIX/bin" ]; then
    echo "postgres prefix: $POSTGRES_PREFIX"
    mkdir -p "$OUT/nix-store" "$OUT/postgresql/bin" "$OUT/bin"
    bases=$(mktemp)
    raw=$(mktemp)
    # ldd of the server, clients, and PostGIS. A full nix-store -qR also
    # pulls compilers, which this image does not need.
    set +e
    {
        for name in postgres initdb psql pg_dump pg_restore pg_ctl; do
            printf '%s\n' "$POSTGRES_PREFIX/bin/$name"
        done
        find "$POSTGRES_PREFIX/lib" -name 'postgis-*.so' -o -name 'postgis.so'
    } | while read -r obj; do
        real=$(readlink -f "$obj" 2>/dev/null) || continue
        [ -f "$real" ] || continue
        printf '%s\n' "$real"
        ldd "$real" 2>/dev/null | awk '/=> \// { print $3 } /ld-linux/ { print $1 }'
    done > "$raw"
    set -e
    while read -r dep; do
        case "$dep" in
            /nix/store/*) ;;
            *) continue ;;
        esac
        [ -e "$dep" ] || continue
        store=${dep#/nix/store/}
        store=${store%%/*}
        printf '%s\n' "/nix/store/$store"
    done < "$raw" | sort -u > "$bases"
    rm -f "$raw"
    # The package directories themselves hold share/extension and the wrappers.
    echo "$POSTGRES_PREFIX" >> "$bases"
    wrapped=$(readlink -f "$POSTGRES_PREFIX/bin/.postgres-wrapped" || true)
    if [ -n "$wrapped" ]; then
        echo "${wrapped%/bin/postgres}" >> "$bases"
    fi
    sort -u "$bases" -o "$bases"
    while read -r store; do
        [ -d "$store" ] || continue
        name=$(basename "$store")
        if [ ! -e "$OUT/nix-store/$name" ]; then
            cp -a "$store" "$OUT/nix-store/$name"
        fi
    done < "$bases"
    base_name=$(basename "${wrapped%/bin/postgres}")
    plug_name=$(basename "$POSTGRES_PREFIX")
    # Copied store dirs keep Nix's read-only mode. The extension links are
    # added only in this pack copy.
    if [ -d "$OUT/nix-store/$base_name" ]; then
        chmod -R u+w "$OUT/nix-store/$base_name/lib" "$OUT/nix-store/$base_name/share" 2>/dev/null || true
    fi
    # The server looks up extensions in its own lib and share, not the plugins output.
    if [ -d "$OUT/nix-store/$base_name/lib" ] && [ -d "$OUT/nix-store/$plug_name/lib" ]; then
        for so in "$OUT/nix-store/$plug_name/lib"/postgis*.so; do
            [ -e "$so" ] || continue
            ln -sfn "/nix/store/$plug_name/lib/$(basename "$so")" \
                "$OUT/nix-store/$base_name/lib/$(basename "$so")"
        done
    fi
    if [ -d "$OUT/nix-store/$base_name/share/postgresql/extension" ] \
        && [ -d "$OUT/nix-store/$plug_name/share/postgresql/extension" ]; then
        cp -a "$OUT/nix-store/$plug_name/share/postgresql/extension"/postgis* \
            "$OUT/nix-store/$base_name/share/postgresql/extension/" 2>/dev/null || true
    fi
    for name in postgres initdb psql pg_dump pg_restore pg_ctl; do
        if [ -e "$POSTGRES_PREFIX/bin/$name" ]; then
            ln -sfn "/nix/store/$plug_name/bin/$name" "$OUT/postgresql/bin/$name"
            cp -L "$POSTGRES_PREFIX/bin/$name" "$BIN/$name"
            chmod 755 "$BIN/$name"
        fi
    done
    rm -f "$bases"
fi

echo "wicket pack: $OUT"
ls -l "$BIN" 2>/dev/null || true
