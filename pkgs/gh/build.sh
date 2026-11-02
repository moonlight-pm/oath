# Output: libexec/gh/gh, static; bin/gh, which turns gh's update notice off (the
# store holds the version, and a newer one is a new pin in pkgs/gh) and names
# itself by its slot path (GH_PATH), so the git credential helper `gh auth
# setup-git` writes outlives the next update; every licence the program carries
# in share/doc/gh; and components.json naming each linked module.
export HOME=/tmp/home GOCACHE=/tmp/gocache GOPATH=/tmp/gopath GOMODCACHE=/tmp/gomod \
	CGO_ENABLED=0 GOSUMDB=off GOPROXY="file://$SRC/gomod" GOFLAGS='-mod=readonly -trimpath'
mkdir -p "$HOME" /tmp/gh /out/bin /out/libexec/gh /out/share/doc/gh
cd /tmp/gh
tar -xzf "$SRC/gh-$VERSION.tar.gz" --no-same-owner --strip-components=1
[ ! -f "$FILES/go.mod" ] || cp "$FILES/go.mod" "$FILES/go.sum" .
go build -ldflags "-s -w -X github.com/cli/cli/v2/internal/build.Version=$VERSION \
	-X github.com/cli/cli/v2/internal/build.Date=0000-00-00" -o /out/libexec/gh/gh ./cmd/gh
cp LICENSE /out/share/doc/gh/
(cd "$GOMODCACHE" && find . \( -path ./cache -o -name testdata \) -prune -o -type f \( -iname 'LICEN[CS]E*' -o -iname 'NOTICE*' \) \
	-exec cp --parents {} /out/share/doc/gh/ \;)
chmod -R u+w "$GOMODCACHE" /out/share/doc/gh
go-components gh "$VERSION" "pkg:golang/github.com/cli/cli/v2@$VERSION" /out/libexec/gh/gh >/out/components.json
cat >/out/bin/gh <<EOT
#!/bin/sh
# GitHub CLI $VERSION. Updates are a new pin in Oath's pkgs/gh, so gh's update
# notice is off.
export GH_NO_UPDATE_NOTIFIER=1 GH_PATH=$LIVE/bin/gh
exec $LIVE/libexec/gh/gh "\$@"
EOT
chmod 755 /out/bin/gh
/out/libexec/gh/gh --version | grep -q "^gh version $VERSION "
