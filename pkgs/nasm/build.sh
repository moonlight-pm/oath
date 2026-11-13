# nasm and ndisasm, on pkg:zlib.
tar -xf "$SRC/$SOURCES"
cd "nasm-$VERSION"
./configure --prefix="$LIVE" --with-zlib=/oath/store/pkg/zlib/live
make -j"$JOBS"
make install DESTDIR=/tmp/dest
mkdir -p /out/bin
cp "/tmp/dest$LIVE/bin/nasm" "/tmp/dest$LIVE/bin/ndisasm" /out/bin/
/out/bin/nasm -v | grep -q "^NASM version $VERSION "
