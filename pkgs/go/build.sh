# Output: the vendor's GOROOT at the pack's root (bin/go, pkg/tool, src),
# without its test suite and test data (ELF fixtures), and
# bin/go-components. The vendor's tools are static, so they need no
# patchelf.
export HOME=/tmp/home GOCACHE=/tmp/gocache GOPATH=/tmp/gopath
mkdir -p "$HOME"
tar -xzf "$SRC/go$VERSION.linux-amd64.tar.gz" --no-same-owner -C /tmp
cd /tmp/go
rm -rf test
find src -type d -name testdata -prune -exec rm -rf {} +

# Roots: pkg:certs' list first, then Go's own search under /etc.
certs=/oath/store/pkg/certs/live/ssl/cert.pem
sed -i "s|^var certFiles = \[\]string{|&\n\t\"$certs\", // Oath's list (pkg:certs)|" \
	src/crypto/x509/root_linux.go
grep -q "$certs" src/crypto/x509/root_linux.go
sed -i 's/^GOTOOLCHAIN=auto$/GOTOOLCHAIN=local/' go.env
grep -qx 'GOTOOLCHAIN=local' go.env

# The go command too, so `go get` reads the same roots.
CGO_ENABLED=0 bin/go build -trimpath -o /tmp/go.new cmd/go
mv /tmp/go.new bin/go

cat >bin/go-components <<'EOT'
#!/bin/sh
# go-components <name> <version> <purl> <binary>...: the components.json of
# a pack of Go programs: the package, the standard library and every module
# the programs link, from the build info Go writes into them, so advisories
# cover each one.
set -eu
name=$1 version=$2 purl=$3
shift 3
{
	go version -m "$@" | awk -F'\t' '
		/^[^\t]/ { sub(/.*: go/, ""); print "stdlib", $0 }
		$2 == "dep" { p = $3; v = $4; print p, v }
		$2 == "=>" { print "drop", p, v; print $3, $4 }'
} | awk -v name="$name" -v version="$version" -v purl="$purl" '
	$1 == "drop" { delete m[$2 " " $3]; next }
	{ m[$1 " " $2] = 1 }
	END {
		printf "[\n  {\"name\": \"%s\", \"version\": \"%s\", \"purl\": \"%s\"}", name, version, purl
		n = asorti(m, s)
		for (i = 1; i <= n; i++) {
			split(s[i], f, " ")
			v = f[2]
			sub(/^v/, "", v)
			printf ",\n  {\"name\": \"%s\", \"version\": \"%s\", \"purl\": \"pkg:golang/%s@%s\"}", f[1], v, f[1], f[2]
		}
		print "\n]"
	}'
EOT
chmod 755 bin/go-components

cp -a . /out/

# It builds a static program and, through gcc, a cgo one; both run, and the
# static one carries the path of the roots.
export PATH="/out/bin:$PATH" GOROOT=/out
cd /tmp
cat >hello.go <<'EOT'
package main

import (
	"crypto/x509"
	"fmt"
)

func main() {
	_, err := x509.SystemCertPool()
	fmt.Println("hello", err)
}
EOT
CGO_ENABLED=0 go build -trimpath -o hello hello.go
./hello | grep -q '^hello'
grep -aq "$certs" hello
go-components hello 1 pkg:generic/hello@1 hello | grep -q "pkg:golang/stdlib@go\?$VERSION\|pkg:golang/stdlib@$VERSION"
cat >hello-cgo.go <<'EOT'
package main

// int two(void) { return 2; }
import "C"
import "fmt"

func main() { fmt.Println(C.two()) }
EOT
CGO_ENABLED=1 go build -trimpath -o hello-cgo hello-cgo.go
[ "$(./hello-cgo)" = 2 ]
[ "$(go env GOTOOLCHAIN)" = local ]
go version | grep -q "go$VERSION "
