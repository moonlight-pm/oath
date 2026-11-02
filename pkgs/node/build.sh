# Output: bin/node, npm and npx, include/node for native add-ons, and share/.
# Oath has no /usr/bin/env, so npm's and npx's #! lines name this pack's node.
# npm's update notice is off: the store holds the version, and a newer npm comes
# with a new pin in pkgs/node.
export HOME=/tmp/home
mkdir -p "$HOME" /out/share/doc/node
tar -xf "$SRC/node-v$VERSION.tar.xz"
cd "node-v$VERSION"
./configure --prefix="$LIVE"
make -j"$JOBS"
make install DESTDIR=/tmp/dest PREFIX="$LIVE"
cp -a "/tmp/dest$LIVE/." /out/
cp LICENSE /out/share/doc/node/
for f in /out/lib/node_modules/npm/bin/npm-cli.js /out/lib/node_modules/npm/bin/npx-cli.js; do
	sed -i "1s|^#!/usr/bin/env node\$|#!$LIVE/bin/node|" "$f"
	head -n 1 "$f" | grep -qx "#!$LIVE/bin/node"
done
# R57: nothing younger than a week, and no package's install scripts. A project
# that needs a script says so in its own .npmrc, and takes the risk knowingly.
printf 'update-notifier=false\nmin-release-age=7\nignore-scripts=true\n' >/out/lib/node_modules/npm/npmrc

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
[ "$($npm config get min-release-age)" = 7 ]
[ "$($npm config get ignore-scripts)" = true ]
