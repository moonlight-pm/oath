# AWS CLI v2 and the Python libraries it needs, in libexec/aws, with
# bin/aws and bin/aws_completer running it on pkg:python, and
# bin/session-manager-plugin, static, which `aws ssm start-session` runs. -I keeps PYTHONPATH
# and a user's site-packages out of what it imports, as a frozen binary would.
# No pip and no setup.py: each sdist's module is copied into place, and the two
# C extensions are compiled here. _awscrt carries its own AWS-LC and s2n, with
# their symbols hidden; every other TLS connection is Python's, on pkg:openssl.
export HOME=/tmp/home
py=/oath/store/pkg/python/live
inc=$py/include/python3.15
site=/out/libexec/aws/site
lic=/out/share/licenses/aws
mkdir -p "$HOME" "$site/ruamel/yaml" "$lic" /out/bin /tmp/u
cd /tmp/u
tar -xf "$SRC/awscli-$VERSION.tar.gz" --no-same-owner
for t in "$SRC"/pypi/*.tar.gz; do
	tar -xf "$t" --no-same-owner
done
tar -xf "$SRC/Python-3.15.2.tar.xz" --no-same-owner Python-3.15.2/Modules/_sqlite
[ "$(python3 -I -c 'import platform; print(platform.python_version())')" = 3.15.2 ]
for p in "$FILES"/patches/*.patch; do
	patch -d "awscli-$VERSION" -p1 -i "$p"
done

cp -a "awscli-$VERSION/awscli" awscrt-*/awscrt colorama-*/colorama \
	distro-*/src/distro docutils-*/docutils jmespath-*/jmespath \
	prompt_toolkit-*/src/prompt_toolkit python-dateutil-*/src/dateutil \
	six-*/six.py urllib3-*/src/urllib3 wcwidth-*/wcwidth "$site/"
cp ruamel.yaml-*/*.py "$site/ruamel/yaml/"
rm "$site/ruamel/yaml/setup.py"
# prompt_toolkit reads its own version from its metadata.
mkdir "$site/prompt_toolkit.egg-info"
cp prompt_toolkit-*/PKG-INFO "$site/prompt_toolkit.egg-info/"

c=$(echo /tmp/u/awscrt-*)
# The sandbox's flags name every pack's headers, pkg:openssl's among them, and
# they come first: the CRT would compile OpenSSL's against AWS-LC's library.
unset CPPFLAGS LDFLAGS PKG_CONFIG_PATH
CFLAGS="-O2 -pipe" CXXFLAGS="-O2 -pipe"
cmake -S "$c/crt" -B /tmp/crt -DCMAKE_INSTALL_PREFIX=/tmp/i \
	-DCMAKE_BUILD_TYPE=Release -DAWS_USE_LIBCRYPTO_TO_SUPPORT_ED25519_EVERYWHERE=ON
cmake --build /tmp/crt --target install -j"$JOBS"
cc -O2 -fPIC -shared -std=gnu99 -Wno-strict-aliasing -DPy_LIMITED_API=0x030D0000 \
	-I"$inc" -I/tmp/i/include "$c"/source/*.c -L/tmp/i/lib -L/tmp/i/lib64 \
	-laws-c-s3 -laws-c-mqtt -laws-c-auth -laws-c-http -laws-c-event-stream \
	-laws-c-compression -laws-checksums -laws-c-io -laws-c-cal -laws-c-sdkutils \
	-laws-c-common -ls2n -lcrypto -lrt -pthread -Wl,--exclude-libs,libcrypto.a \
	-o "$site/_awscrt.abi3.so"
# awscli's startup imports sqlite3, which pkg:python does not build. Adding it
# there would rebuild everything built with Python, glibc included.
s=Python-3.15.2/Modules/_sqlite
cc -O2 -fPIC -shared -I"$inc" -I"$inc/internal" -I"$inc/internal/mimalloc" -I"$s" \
	-I/oath/store/pkg/sqlite/live/include "$s"/*.c \
	/oath/store/pkg/sqlite/live/lib/libsqlite3.a -lm \
	-o "$site/_sqlite3.cpython-315-x86_64-linux-gnu.so"
find "$site" -name '*.so' -exec strip --strip-debug {} +

# botocore's roots are pkg:certs', for Python's TLS and the CRT's alike.
ln -sf /oath/store/pkg/certs/live/ssl/cert.pem "$site/awscli/botocore/cacert.pem"
python3 -I -B -c "
import sys
sys.path.insert(0, '$site')
from awscli.autocomplete.generator import generate_index
generate_index('$site/awscli/data/ac.index')
"

for b in aws aws_completer; do
	{
		echo "#!$py/bin/python3 -I"
		echo 'import sys'
		echo "sys.path.insert(0, '$LIVE/libexec/aws/site')"
		sed 1d "awscli-$VERSION/bin/$b"
	} >"/out/bin/$b"
	chmod 755 "/out/bin/$b"
done

# The Session Manager plugin, its modules from their pins and its vendor/ left
# in the tarball. The source's version.go says 1.3.0.0; AWS's release sets it.
smp=$(basename "$SRC"/session-manager-plugin-*.tar.gz .tar.gz)
smv=${smp#session-manager-plugin-}
mkdir "/tmp/$smp" "$lic/session-manager-plugin"
(
	export GOCACHE=/tmp/gocache GOPATH=/tmp/gopath GOMODCACHE=/tmp/gomod CGO_ENABLED=0 \
		GOSUMDB=off GOPROXY="file://$SRC/gomod" GOFLAGS='-mod=readonly -trimpath'
	cd "/tmp/$smp"
	tar -xzf "$SRC/$smp.tar.gz" --no-same-owner --strip-components=1 --exclude='*/vendor'
	for p in "$FILES"/patches/go/*.patch; do
		patch -p1 -i "$p"
	done
	cp "$FILES/go.mod" "$FILES/go.sum" .
	sed -i "s/^const Version = .*/const Version = \"$smv\"/" src/version/version.go
	go build -ldflags "-s -w" -o /out/bin/session-manager-plugin ./src/sessionmanagerplugin-main
	cp LICENSE NOTICE THIRD-PARTY "$lic/session-manager-plugin/"
	(cd "$GOMODCACHE" && find . \( -path ./cache -o -name testdata \) -prune -o -type f \( -iname 'LICEN[CS]E*' -o -iname 'NOTICE*' \) \
		-exec cp --parents {} "$lic/session-manager-plugin/" \;)
	chmod -R u+w "$GOMODCACHE" "$lic/session-manager-plugin"
	go-components session-manager-plugin "$smv" \
		"pkg:golang/github.com/aws/session-manager-plugin@$smv" \
		/out/bin/session-manager-plugin >/tmp/smp-components.json
)

for d in "awscli-$VERSION" awscrt-* colorama-* distro-* docutils-* jmespath-* \
	prompt_toolkit-* python-dateutil-* ruamel.yaml-* six-* urllib3-* wcwidth-* \
	awscrt-*/crt/aws-lc awscrt-*/crt/s2n; do
	n=$(basename "$d")
	n=${n%-[0-9]*}
	mkdir -p "$lic/$n"
	find "$d" -maxdepth 1 -type f \( -name 'LICENSE*' -o -name 'COPYING*' -o -name 'NOTICE*' \) \
		-exec cp -t "$lic/$n/" {} +
	[ -n "$(ls "$lic/$n")" ]
done

python3 -I -B - "$VERSION" "$SRC" "$c" >/out/components.json <<'EOT'
import glob, json, os, re, sys
version, src, crt = sys.argv[1:]
out = [{"name": "aws-cli", "version": version, "purl": f"pkg:github/aws/aws-cli@{version}"}]
for path in sorted(glob.glob(src + "/pypi/*.tar.gz")):
    name, v = re.fullmatch(r"(.+)-([^-]+)\.tar\.gz", os.path.basename(path)).groups()
    name = re.sub(r"[-_.]+", "-", name).lower()
    out.append({"name": name, "version": v, "purl": f"pkg:pypi/{name}@{v}"})
lc = re.search(r'AWSLC_VERSION_NUMBER_STRING "([0-9.]+)"', open(crt + "/crt/aws-lc/include/openssl/base.h").read()).group(1)
out.append({"name": "aws-lc", "version": lc, "purl": f"pkg:generic/aws-lc@{lc}"})
out.extend(json.load(open("/tmp/smp-components.json")))
print(json.dumps(out, indent=2))
EOT
python3 -I -m compileall -q -j "$JOBS" --invalidation-mode unchecked-hash -s /out -p "$LIVE" "$site"

# It starts on this Python, its C extensions load, and help renders.
sed "s#$LIVE/libexec#/out/libexec#" /out/bin/aws >/tmp/aws
python3 -I -B /tmp/aws --version | grep -qF "aws-cli/$VERSION Python/3.15.2"
PAGER=cat python3 -I -B /tmp/aws sts get-caller-identity help | grep -q 'get-caller-identity'
python3 -I -B -c "
import sys, hashlib
sys.path.insert(0, '$site')
import sqlite3, ssl, urllib3
from awscrt.crypto import EC, ECType
assert ssl.OPENSSL_VERSION.startswith('OpenSSL 4.')
d = hashlib.sha256(b'x').digest()
k = EC.new_generate(ECType.P_256)
assert k.verify(d, k.sign(d))
sqlite3.connect('$site/awscli/data/ac.index').close()
"
/out/bin/session-manager-plugin --version | grep -qx "$smv"
/out/bin/session-manager-plugin | grep -q 'installed successfully'
