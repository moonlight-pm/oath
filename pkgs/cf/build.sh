# Every tarball in libexec/cf/node_modules as npm lays it out (no two
# packages need different versions of a third, so the tree is flat), and
# bin/cf and bin/cloudflare, the names npm would link, which run cf with
# pkg:node. npm never runs, and nor do install scripts: workerd's only
# checks that its platform package, which is here, is installed. workerd
# names /lib64/ld-linux-x86-64.so.2; Oath's loader is /lib's (R17), so
# patchelf points it there. components.json names every package, so
# advisories cover each one.
export HOME=/tmp/home
m=/out/libexec/cf/node_modules
mkdir -p "$HOME" "$m" /out/bin
for t in $(cd "$SRC" && find npm -name '*.tgz' | sort); do
	rm -rf /tmp/p && mkdir /tmp/p
	tar -xzf "$SRC/$t" --no-same-owner -C /tmp/p
	set -- /tmp/p/*
	[ $# = 1 ] && [ -f "$1/package.json" ]
	n=$(node -p 'require(process.argv[1]).name' "$1/package.json")
	[ ! -e "$m/$n" ]
	mkdir -p "$(dirname "$m/$n")"
	mv "$1" "$m/$n"
done
w=$m/@cloudflare/workerd-linux-64/bin/workerd
patchelf --set-interpreter /lib/ld-linux-x86-64.so.2 "$w"
node - "$m" >/out/components.json <<'EOT'
const fs = require("fs"), path = require("path");
const m = process.argv[2], out = [];
for (const d of fs.readdirSync(m).sort()) {
	const ds = d.startsWith("@") ? fs.readdirSync(path.join(m, d)).sort().map((s) => `${d}/${s}`) : [d];
	for (const n of ds) {
		const p = JSON.parse(fs.readFileSync(path.join(m, n, "package.json")));
		out.push({ name: p.name, version: p.version, purl: `pkg:npm/${p.name.replace(/^@/, "%40")}@${p.version}` });
	}
}
console.log(JSON.stringify(out, null, 2));
EOT
for b in cf cloudflare; do
	cat >/out/bin/$b <<EOT
#!/bin/sh
# cf $VERSION, Cloudflare's CLI, with the npm packages pkgs/cf pins; a
# newer one is a new set of pins there.
exec /oath/store/pkg/node/live/bin/node $LIVE/libexec/cf/node_modules/cf/bin/cf "\$@"
EOT
	chmod 755 /out/bin/$b
done

# It runs; miniflare loads (sharp and its libvips with it), and workerd
# starts.
grep -q '"name": "cf"' /out/components.json
node "$m/cf/bin/cf" --version | grep -qF "$VERSION"
node --input-type=module -e "await import('$m/miniflare/dist/src/index.js'); await import('$m/sharp/dist/index.mjs')"
"$w" --version | grep -q '^workerd '
