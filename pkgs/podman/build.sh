# Output: bin/podman, libexec/podman/{rootlessport,quadlet}, the defaults in
# share/containers, every licence the program carries in share/doc/podman,
# and components.json naming each linked module. A machine's own settings go
# in /etc/containers, which it reads over these.
export HOME=/tmp/home GOCACHE=/tmp/gocache GOPATH=/tmp/gopath GOMODCACHE=/tmp/gomod \
	CGO_ENABLED=1 GOSUMDB=off GOPROXY="file://$SRC/gomod" GOFLAGS='-mod=readonly -trimpath' \
	CGO_CFLAGS="$CFLAGS" CGO_LDFLAGS="$LDFLAGS"
mkdir -p "$HOME" /tmp/podman /out/bin /out/libexec/podman /out/share/containers /out/share/doc/podman
cd /tmp/podman
tar -xzf "$SRC/podman-$VERSION.tar.gz" --no-same-owner --strip-components=1
for p in "$FILES"/patches/*.patch; do
	patch -p1 -i "$p"
done
rm -rf vendor
cp "$FILES/go.mod" "$FILES/go.sum" .

tags='grpcnotrace seccomp libsqlite3 containers_image_openpgp exclude_graphdriver_btrfs'
p=github.com/containers/podman/v6
# libsqlite3.a needs libm, after it on the linker's line.
ld="-s -w -extldflags=-lm -X $p/libpod/config._installPrefix=$LIVE -X $p/libpod/config._etcDir=/etc
	-X $p/pkg/systemd/quadlet._binDir=$LIVE/bin
	-X go.podman.io/common/pkg/config.additionalHelperBinariesDir=$LIVE/libexec/podman
	-X go.podman.io/storage/pkg/configfile.systemConfigPath=$LIVE/share/containers"
go build -tags "$tags" -ldflags "$ld" -o /out/bin/podman ./cmd/podman
go build -tags "$tags" -ldflags "$ld" -o /out/libexec/podman/rootlessport ./cmd/rootlessport
go build -tags "$tags" -ldflags "$ld" -o /out/libexec/podman/quadlet ./cmd/quadlet

cp "$FILES/policy.json" "$FILES/containers.conf" "$FILES/storage.conf" /out/share/containers/
cp "$FILES/netavark" /out/libexec/podman/netavark
chmod 755 /out/libexec/podman/netavark
cp LICENSE /out/share/doc/podman/
(cd "$GOMODCACHE" && find . \( -path ./cache -o -name testdata \) -prune -o -type f \( -iname 'LICEN[CS]E*' -o -iname 'NOTICE*' \) \
	-exec cp --parents {} /out/share/doc/podman/ \;)
chmod -R u+w "$GOMODCACHE" /out/share/doc/podman
go-components podman "$VERSION" "pkg:golang/$p@$VERSION" \
	/out/bin/podman /out/libexec/podman/rootlessport /out/libexec/podman/quadlet >/out/components.json

/out/bin/podman --version | grep -q "version $VERSION"
