# The vendor's binary in libexec/, its loader pointed at Oath's, and bin/bun,
# which holds every install to R57: nothing younger than a week, and no
# lifecycle script. No bunx: the programs it runs start #!/usr/bin/env, and
# Oath has no /usr. `bun upgrade` cannot write the store: a newer Bun is a
# new pin in pkgs/bun.
export HOME=/tmp/home
mkdir -p "$HOME" /out/bin /out/libexec/bun /out/share/doc/bun
b=/out/libexec/bun/bun
python3 -c 'import shutil, sys, zipfile
with zipfile.ZipFile(sys.argv[1]) as z, z.open("bun-linux-x64-baseline/bun") as f, open(sys.argv[2], "wb") as o:
    shutil.copyfileobj(f, o)' "$SRC/bun-linux-x64-baseline.zip" "$b"
# The ELF's .interp, at the start of the file, names
# /lib64/ld-linux-x86-64.so.2; Oath's loader is /lib's (R17). As for Claude
# Code, the shorter name is written over the old one, NUL-padded, and
# nothing in the file moves. `bun build --compile` copies this binary, so
# what it makes runs on Oath.
off=$(grep -obUaF /lib64/ld-linux-x86-64.so.2 "$b" | head -n 1 | cut -d: -f1)
[ "$off" -lt 4096 ]
printf '/lib/ld-linux-x86-64.so.2\0\0\0' | dd of="$b" bs=1 seek="$off" conv=notrunc status=none
chmod 755 "$b"
cp "$SRC/bun-LICENSE.md" /out/share/doc/bun/LICENSE.md
# Bun reads BUN_OPTIONS before the command, as if typed first, and every bun
# it starts (an install's, a script's) inherits it. Its flags outrank a
# project's bunfig.toml and .npmrc; only a flag typed after them could
# loosen one, so the age flag is refused there.
cat >/out/bin/bun <<EOF
#!/bin/sh
# Bun $VERSION. On Oath every install takes nothing published less than a
# week ago and runs no lifecycle script, not even the project's own (R57).
for a; do
	case \$a in
	--minimum-release-age*)
		echo "bun: \$a refused: on Oath every install takes nothing younger than a week" >&2
		exit 1
		;;
	esac
done
case \${1-} in
x)
	echo "bun x: refused: a package's program starts #!/usr/bin/env, and Oath has no /usr; bun add it, then run its file with bun" >&2
	exit 1
	;;
upgrade)
	echo "bun upgrade: refused: a newer Bun is a new pin in Oath's pkgs/bun" >&2
	exit 1
	;;
esac
o="--ignore-scripts --minimum-release-age=604800"
case \${BUN_OPTIONS-} in
*"\$o") ;;
*) BUN_OPTIONS="\${BUN_OPTIONS:+\$BUN_OPTIONS }\$o" ;;
esac
export BUN_OPTIONS
exec $LIVE/libexec/bun/bun "\$@"
EOF
chmod 755 /out/bin/bun

# It runs, with its crypto and its roots.
"$b" --version | grep -qx "$VERSION"
"$b" -e '
const h = new Bun.CryptoHasher("sha256").digest("hex");
if (h !== "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855") process.exit(1);
if (!require("tls").rootCertificates.length) process.exit(1);
'
# bin/bun, pointed at this tree: a project's own postinstall runs under the
# bare binary and not under bin/bun (an install with no dependencies needs
# no network), and what it refuses, it refuses.
w=/tmp/bun-here
sed "s|$LIVE/|/out/|" /out/bin/bun >"$w"
mkdir /tmp/p
cd /tmp/p
printf '{"scripts":{"postinstall":"touch ran"}}' >package.json
"$b" install
rm ran
sh "$w" install
[ ! -e ran ]
for args in 'add left-pad --minimum-release-age=0' 'x cowsay' upgrade; do
	sh "$w" $args 2>/dev/null && exit 1
	true
done
