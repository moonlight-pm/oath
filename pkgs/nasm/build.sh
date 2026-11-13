# nasm and ndisasm, on pkg:zlib. patches/: upstream's fix for CVE-2026-6067
# (too many segments in an OBJ group), which no release has yet.
tar -xf "$SRC/$SOURCES"
cd "nasm-$VERSION"
for p in "$FILES"/patches/*.patch; do
	patch -p1 -i "$p"
done
./configure --prefix="$LIVE" --with-zlib=/oath/store/pkg/zlib/live
make -j"$JOBS"
make install DESTDIR=/tmp/dest
mkdir -p /out/bin
cp "/tmp/dest$LIVE/bin/nasm" "/tmp/dest$LIVE/bin/ndisasm" /out/bin/
/out/bin/nasm -v | grep -q "^NASM version $VERSION "
