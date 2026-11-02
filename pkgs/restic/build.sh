# Output: bin/restic, static, with every licence the program carries in
# share/doc/restic, and components.json naming each linked module.
export HOME=/tmp/home GOCACHE=/tmp/gocache GOPATH=/tmp/gopath GOMODCACHE=/tmp/gomod \
	CGO_ENABLED=0 GOSUMDB=off GOPROXY="file://$SRC/gomod" GOFLAGS='-mod=readonly -trimpath'
mkdir -p "$HOME" /tmp/restic /out/bin /out/share/doc/restic
cd /tmp/restic
tar -xzf "$SRC/restic-$VERSION.tar.gz" --no-same-owner --strip-components=1
for p in "$FILES"/patches/*.patch; do
	patch -p1 -i "$p"
done
cp "$FILES/go.mod" "$FILES/go.sum" .
go build -ldflags "-s -w -X main.version=$VERSION" -o /out/bin/restic ./cmd/restic
cp LICENSE /out/share/doc/restic/
(cd "$GOMODCACHE" && find . \( -path ./cache -o -name testdata \) -prune -o -type f \( -iname 'LICEN[CS]E*' -o -iname 'NOTICE*' \) \
	-exec cp --parents {} /out/share/doc/restic/ \;)
chmod -R u+w "$GOMODCACHE" /out/share/doc/restic
go-components restic "$VERSION" "pkg:golang/github.com/restic/restic@$VERSION" \
	/out/bin/restic >/out/components.json

/out/bin/restic version | grep -q "^restic $VERSION "
export RESTIC_PASSWORD=x RESTIC_REPOSITORY=/tmp/repo RESTIC_CACHE_DIR=/tmp/rcache
/out/bin/restic init
echo data >/tmp/data
/out/bin/restic backup /tmp/data
/out/bin/restic restore latest --target /tmp/restored
cmp /tmp/data /tmp/restored/tmp/data
