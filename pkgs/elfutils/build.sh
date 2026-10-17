# libelf, shared, for the kernel's objtool and resolve_btfids. glibc has
# argp, fts and obstack, so elfutils configures as upstream expects; only
# libelf (and the helpers it folds in) is built.
tar -xf "$SRC/$SOURCES"
cd "elfutils-$VERSION"
./configure --prefix="$LIVE" --disable-nls --disable-debuginfod --disable-libdebuginfod \
	--disable-demangler --without-bzlib --without-lzma --without-zstd
make -C lib -j"$JOBS"
make -C libelf -j"$JOBS"
make -C libelf install DESTDIR=/tmp/dest
make -C config install DESTDIR=/tmp/dest
mkdir -p /out/lib/pkgconfig
cp -a "/tmp/dest$LIVE/include" /out/
cp -a "/tmp/dest$LIVE"/lib/libelf* /out/lib/
cp "/tmp/dest$LIVE/lib/pkgconfig/libelf.pc" /out/lib/pkgconfig/
rm -f /out/lib/*.a
