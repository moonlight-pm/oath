#!/bin/sh
# Pins the modules of a Go package (R49): the go.mod and zip files the go
# command reads to build it, as proxy.golang.org serves them. They replace
# what follows the marker line in the package's pkg.toml, and its build reads
# them through GOPROXY=file://$SRC/gomod. The go command checks each one
# against go.sum and Go's checksum database here, and against go.sum again in
# the build's sandbox.
#
# A module a release pins behind its fix goes in pkgs/<name>/go-updates.txt,
# one `module version` a line, with `#` comments. `go get` takes them here,
# and the resulting go.mod and go.sum go in pkgs/<name>/, which the build
# copies over the release's.
#
# Usage, with the release in build/fetch (cargo make pkgs --fetch <name>) and
# the locked pkg:go built:
#   pkgs/go-modules.sh [--cgo] [--tags 'build tags'] <name> <go package>...
#   pkgs/go-modules.sh restic ./cmd/restic
set -eu
cgo=0 tags=
while :; do
	case ${1:-} in
	--cgo) cgo=1 && shift ;;
	--tags) tags=$2 && shift 2 ;;
	*) break ;;
	esac
done
name=$1
shift
repo=$(cd "$(dirname "$0")/.." && pwd)
toml=$repo/pkgs/$name/pkg.toml
product() {
	awk -v n="  \"$1\": {" 'index($0, n) == 1 { f = 1 } f && /"product"/ { gsub(/[",]/, "", $2); print $2; exit }' "$repo/pkgs/lock.json"
}
store=$HOME/.local/share/oath/store/pkg
export PATH="$store/go/$(product go)/bin:$PATH"
# The roots of the build store's pkg:certs, which need not be live here.
export SSL_CERT_FILE="${SSL_CERT_FILE:-$store/certs/$(product certs)/ssl/cert.pem}"
marker='# Modules, from pkgs/go-modules.sh:'
grep -qx "$marker" "$toml"
# The release is the source just before the marker.
head=$(awk -v m="$marker" '{ print } $0 == m { exit }' "$toml")
file=$(printf '%s\n' "$head" | sed -n 's/^file = "\(.*\)"$/\1/p' | tail -n 1)
[ -n "$file" ] || file=$(printf '%s\n' "$head" | sed -n 's/^url = ".*\/\([^/"]*\)"$/\1/p' | tail -n 1)

tmp=$(mktemp -d)
trap 'chmod -R u+w "$tmp"; rm -rf "$tmp"' EXIT
mkdir "$tmp/src"
tar -xf "$repo/build/fetch/$file" -C "$tmp/src"
# A release with no go.mod of its own builds from the one kept here.
if [ -z "$(find "$tmp/src" -maxdepth 2 -name go.mod)" ]; then
	for f in go.mod go.sum; do
		[ ! -f "$repo/pkgs/$name/$f" ] || cp "$repo/pkgs/$name/$f" "$tmp/src"/*/
	done
fi
cd "$tmp/src/$(cd "$tmp/src" && dirname "$(find . -maxdepth 2 -name go.mod | sort | head -n 1)")"
# The package's patches can change what the build reads. One with more than a
# Go program in it keeps that program's in patches/go.
patches=$repo/pkgs/$name/patches
[ ! -d "$patches/go" ] || patches=$patches/go
for p in "$patches"/*.patch; do
	[ ! -f "$p" ] || git apply -p1 "$p"
done
# A replace by version comes through the proxy and go.sum like any other module.
# One by path names a tree no pin holds.
if grep -Eq '=> *(\.\.?)?/' go.mod; then
	echo "$name: go.mod replaces a module with a path, which this does not follow" >&2
	exit 1
fi

# GOPATH holds only what the go command remembers of the checksum database, kept
# between runs so it can notice a log rolled back; its default is ~/go.
export GOTOOLCHAIN=local GOFLAGS=-mod=readonly CGO_ENABLED=$cgo GOOS=linux GOARCH=amd64 \
	GOPROXY=https://proxy.golang.org GOSUMDB=sum.golang.org GOMODCACHE="$tmp/mod" \
	GOPATH="$HOME/.local/share/go"
updates=$repo/pkgs/$name/go-updates.txt
if [ -f "$updates" ]; then
	# All at once: asked for one by one, a lower version of one module would
	# silently downgrade another that needs it higher. Together, it is an error.
	# shellcheck disable=SC2046
	GOFLAGS=-mod=mod go get $(sed -e 's/#.*//' -e '/^ *$/d' "$updates" | awk '{print $1 "@" $2}')
	GOFLAGS=-mod=mod go list -deps -tags "$tags" "$@" >/dev/null
	cp go.mod go.sum "$repo/pkgs/$name/"
	chmod -R u+w "$GOMODCACHE"
	rm -rf "$GOMODCACHE"
fi

# From an empty cache, what the go command fetches is what the build reads.
go list -deps -tags "$tags" "$@" >/dev/null
cache=$GOMODCACHE/cache/download
printf '%s\n' "$head" >"$tmp/toml"
(cd "$cache" && find . -path ./sumdb -prune -o -type f \( -name '*.mod' -o -name '*.zip' \) -print) |
	LC_ALL=C sort | while read -r f; do
	f=${f#./}
	printf '\n[[source]]\nurl = "https://proxy.golang.org/%s"\nsha256 = "%s"\nfile = "gomod/%s"\n' \
		"$f" "$(sha256sum "$cache/$f" | cut -d' ' -f1)" "$f"
done >>"$tmp/toml"
cp "$tmp/toml" "$toml"
echo "$name: $(grep -c '^file = "gomod/' "$toml") modules' files pinned"
