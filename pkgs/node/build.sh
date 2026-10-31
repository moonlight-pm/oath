# The vendor's tree as it ships: bin/node, npm and npx, include/node for
# native add-ons, and share/. Oath has no /lib64 (R17), so patchelf points
# node at /lib's loader; nor /usr/bin/env, so npm's and npx's #! lines
# name this pack's node. npm's update notice is off: the store holds the
# version, and a newer npm comes with a new pin in pkgs/node.
export HOME=/tmp/home
mkdir -p "$HOME" /out/share/doc/node
tar -xf "$SRC/node-v$VERSION-linux-x64.tar.xz"
cd "node-v$VERSION-linux-x64"
cp -R bin include lib share /out/
cp LICENSE /out/share/doc/node/
patchelf --set-interpreter /lib/ld-linux-x86-64.so.2 /out/bin/node
for f in /out/lib/node_modules/npm/bin/npm-cli.js /out/lib/node_modules/npm/bin/npx-cli.js; do
	sed -i "1s|^#!/usr/bin/env node\$|#!$LIVE/bin/node|" "$f"
	head -n 1 "$f" | grep -qx "#!$LIVE/bin/node"
done
echo 'update-notifier=false' >/out/lib/node_modules/npm/npmrc

# It runs, with its TLS and crypto; npm reads its built-in config.
/out/bin/node --version | grep -qx "v$VERSION"
/out/bin/node -e '
const h = require("crypto").createHash("sha256").digest("hex");
if (h !== "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855") process.exit(1);
if (!require("tls").rootCertificates.length) process.exit(1);
'
npm="/out/bin/node /out/lib/node_modules/npm/bin/npm-cli.js"
$npm --version
[ "$($npm config get update-notifier)" = false ]
