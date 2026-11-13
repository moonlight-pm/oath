# bin/bun, the vendor's binary with its loader pointed at Oath's, and
# bin/bunx, which Bun answers as `bun x` by the name it was run as. `bun
# upgrade` cannot write the store: a newer Bun is a new pin in pkgs/bun.
export HOME=/tmp/home
mkdir -p "$HOME" /out/bin /out/share/doc/bun
b=/out/bin/bun
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
ln -s bun /out/bin/bunx
cp "$SRC/bun-LICENSE.md" /out/share/doc/bun/LICENSE.md

# It runs, with its crypto and its roots, and bunx is `bun x`.
"$b" --version | grep -qx "$VERSION"
"$b" -e '
const h = new Bun.CryptoHasher("sha256").digest("hex");
if (h !== "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855") process.exit(1);
if (!require("tls").rootCertificates.length) process.exit(1);
'
/out/bin/bunx --help 2>&1 | grep -q '^Usage: bunx '
