# Output: bin/cell, static, and components.json naming the Go it links. Running a
# container needs a user namespace, which the sandbox blocks: that is tested on a machine.
export HOME=/tmp/home GOCACHE=/tmp/gocache GOPATH=/tmp/gopath CGO_ENABLED=0 GOPROXY=off \
	GOFLAGS='-mod=readonly -trimpath'
mkdir -p "$HOME" /out/bin
cp -a "$SRC/cell" /tmp/cell
cd /tmp/cell
go vet ./...
go test ./...
go build -ldflags '-s -w' -o /out/bin/cell .
go-components cell "$VERSION" "pkg:generic/cell@$VERSION" /out/bin/cell >/out/components.json
CELL_ROOT=/tmp/store /out/bin/cell images
