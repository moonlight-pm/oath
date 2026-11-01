# Output: bin/blocky, static, with every licence the program carries in
# share/doc/blocky, and components.json naming each linked module.
export HOME=/tmp/home GOCACHE=/tmp/gocache GOPATH=/tmp/gopath GOMODCACHE=/tmp/gomod \
	CGO_ENABLED=0 GOSUMDB=off GOPROXY="file://$SRC/gomod" GOFLAGS='-mod=readonly -trimpath'
mkdir -p "$HOME" /tmp/blocky /out/bin /out/share/doc/blocky
cd /tmp/blocky
tar -xzf "$SRC/blocky-$VERSION.tar.gz" --no-same-owner --strip-components=1
cp "$FILES/go.mod" "$FILES/go.sum" .
go build -ldflags "-s -w \
	-X github.com/0xERR0R/blocky/util.Version=v$VERSION \
	-X github.com/0xERR0R/blocky/util.BuildTime=00000000-000000 \
	-X github.com/0xERR0R/blocky/util.Architecture=amd64" -o /out/bin/blocky .
cp LICENSE /out/share/doc/blocky/
(cd "$GOMODCACHE" && find . \( -path ./cache -o -name testdata \) -prune -o -type f \( -iname 'LICEN[CS]E*' -o -iname 'NOTICE*' \) \
	-exec cp --parents {} /out/share/doc/blocky/ \;)
chmod -R u+w "$GOMODCACHE" /out/share/doc/blocky
go-components blocky "$VERSION" "pkg:golang/github.com/0xERR0R/blocky@$VERSION" \
	/out/bin/blocky >/out/components.json

/out/bin/blocky version | grep -q "v$VERSION"
cat >/tmp/blocky.yml <<'EOT'
upstreams:
  init:
    strategy: fast
  groups:
    default:
      - 127.0.0.1
ports:
  dns: 127.0.0.1:5353
EOT
/out/bin/blocky validate --config /tmp/blocky.yml
