# Output: bin/caddy, static, with every licence the program carries in
# share/doc/caddy, and components.json naming each linked module.
export HOME=/tmp/home GOCACHE=/tmp/gocache GOPATH=/tmp/gopath GOMODCACHE=/tmp/gomod \
	CGO_ENABLED=0 GOSUMDB=off GOPROXY="file://$SRC/gomod" GOFLAGS='-mod=readonly -trimpath'
mkdir -p "$HOME" /tmp/caddy /out/bin /out/share/doc/caddy
cd /tmp/caddy
tar -xzf "$SRC/caddy_${VERSION}_buildable-artifact.tar.gz" --no-same-owner
rm -rf vendor
cp "$FILES/go.mod" "$FILES/go.sum" .
go build -tags 'nobadger nomysql nopgx' -ldflags '-s -w' -o /out/bin/caddy .
cp LICENSE /out/share/doc/caddy/
(cd "$GOMODCACHE" && find . \( -path ./cache -o -name testdata \) -prune -o -type f \( -iname 'LICEN[CS]E*' -o -iname 'NOTICE*' \) \
	-exec cp --parents {} /out/share/doc/caddy/ \;)
chmod -R u+w "$GOMODCACHE" /out/share/doc/caddy
go-components caddy "$VERSION" "pkg:golang/github.com/caddyserver/caddy/v2@$VERSION" \
	/out/bin/caddy >/out/components.json

/out/bin/caddy version | grep -q "^v$VERSION"
cat >/tmp/Caddyfile <<'EOT'
:8080
respond "ok"
EOT
/out/bin/caddy validate --adapter caddyfile --config /tmp/Caddyfile
