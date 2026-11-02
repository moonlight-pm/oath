# Output: bin/route, static, and components.json naming the Go it links.
export HOME=/tmp/home GOCACHE=/tmp/gocache GOPATH=/tmp/gopath CGO_ENABLED=0 GOPROXY=off \
	GOFLAGS='-mod=readonly -trimpath'
mkdir -p "$HOME" /out/bin
cp -a "$SRC/route" /tmp/route
cd /tmp/route
go vet ./...
go test ./...
go build -ldflags '-s -w' -o /out/bin/route .
go-components route "$VERSION" "pkg:generic/route@$VERSION" /out/bin/route >/out/components.json
printf '{"listen": ":0", "routes": [{"host": "a", "status": 200}]}' >/tmp/ok.json
/out/bin/route -check /tmp/ok.json | grep -q '1 routes'
