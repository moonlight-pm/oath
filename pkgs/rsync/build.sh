# Output: bin/rsync. popt is the copy rsync carries, for one tool's options;
# no international domain names, which would need libidn2.
tar -xf "$SRC/$SOURCES"
cd "rsync-$VERSION"
./configure --prefix="$LIVE" --with-included-popt --with-included-zlib=no --disable-acl-support \
	--disable-lz4 --disable-xxhash --disable-md2man --disable-roll-simd --disable-idn
make -j"$JOBS"
make install-strip DESTDIR=/tmp/dest
mkdir -p /out/bin
cp "/tmp/dest$LIVE/bin/rsync" /out/bin/

/out/bin/rsync --version | grep -q "version $VERSION"
/out/bin/rsync --version | grep -q 'zstd'
mkdir -p /tmp/a/b
echo data >/tmp/a/b/c
/out/bin/rsync -a --checksum --checksum-choice=sha1 /tmp/a/ /tmp/z/
cmp /tmp/a/b/c /tmp/z/b/c
