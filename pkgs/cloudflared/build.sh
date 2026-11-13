# Output: bin/cloudflared, static, with every licence the program carries in
# share/doc/cloudflared, and components.json naming each linked module.
# BuiltForPackageManager is upstream's switch for a packaged build: no
# self-update, and `cloudflared update` says to use the package manager. A newer
# cloudflared is a new pin in pkgs/cloudflared. patches/: upstream's fix of
# VULN-170057 (an Access bypass through matrix parameters), in 2026.11.0, which is
# under a week old; the last check below is that fix.
export HOME=/tmp/home GOCACHE=/tmp/gocache GOPATH=/tmp/gopath GOMODCACHE=/tmp/gomod \
	CGO_ENABLED=0 GOSUMDB=off GOPROXY="file://$SRC/gomod" GOFLAGS='-mod=readonly -trimpath'
mkdir -p "$HOME" /tmp/cloudflared /out/bin /out/share/doc/cloudflared
cd /tmp/cloudflared
tar -xzf "$SRC/cloudflared-$VERSION.tar.gz" --no-same-owner --strip-components=1
for p in "$FILES"/patches/*.patch; do
	[ -f "$p" ] || continue
	patch -p1 -i "$p"
done
[ ! -f "$FILES/go.mod" ] || cp "$FILES/go.mod" "$FILES/go.sum" .
go build -ldflags "-s -w -X main.Version=$VERSION \
	-X github.com/cloudflare/cloudflared/cmd/cloudflared/updater.BuiltForPackageManager=oath" \
	-o /out/bin/cloudflared ./cmd/cloudflared
cp LICENSE /out/share/doc/cloudflared/
(cd "$GOMODCACHE" && find . \( -path ./cache -o -name testdata \) -prune -o -type f \( -iname 'LICEN[CS]E*' -o -iname 'COPYING*' -o -iname 'NOTICE*' \) \
	-exec cp --parents {} /out/share/doc/cloudflared/ \;)
chmod -R u+w "$GOMODCACHE" /out/share/doc/cloudflared
go-components cloudflared "$VERSION" "pkg:golang/github.com/cloudflare/cloudflared@$VERSION" \
	/out/bin/cloudflared >/out/components.json

/out/bin/cloudflared --version | grep -q "version $VERSION "
/out/bin/cloudflared update 2>&1 | grep -q 'installed by oath'
cat >/tmp/cloudflared.yml <<'EOT'
ingress:
  - hostname: example.com
    service: http://127.0.0.1:8080
  - service: http_status:404
EOT
/out/bin/cloudflared tunnel --config /tmp/cloudflared.yml ingress validate
cat >/tmp/cloudflared-access.yml <<'EOT'
ingress:
  - hostname: example.com
    path: ^/admin
    service: http://127.0.0.1:8081
    originRequest:
      access:
        required: true
        teamName: example
        audTag: [example]
  - hostname: example.com
    service: http://127.0.0.1:8080
  - service: http_status:404
EOT
/out/bin/cloudflared tunnel --config /tmp/cloudflared-access.yml ingress rule \
	'https://example.com/public/.;x/../admin' | grep -q 'Matched rule #0'
