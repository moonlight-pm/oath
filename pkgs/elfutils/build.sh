# libelf, shared, for the kernel's objtool and resolve_btfids. glibc has
# argp, fts and obstack, so elfutils configures as upstream expects; only
# libelf (and the helpers it folds in) is built.
tar -xf "$SRC/$SOURCES"
cd "elfutils-$VERSION"
./configure --prefix="$LIVE" --libdir="$LIVE/$LIBDIR" --disable-nls --disable-debuginfod --disable-libdebuginfod \
	--disable-demangler --without-bzlib --without-lzma --without-zstd
make -C lib -j"$JOBS"
make -C libelf -j"$JOBS"
make -C libelf install DESTDIR=/tmp/dest
make -C config install DESTDIR=/tmp/dest
mkdir -p "/out/$LIBDIR/pkgconfig"
cp -a "/tmp/dest$LIVE/include" /out/
cp -a "/tmp/dest$LIVE/$LIBDIR"/libelf* "/out/$LIBDIR/"
cp "/tmp/dest$LIVE/$LIBDIR/pkgconfig/libelf.pc" "/out/$LIBDIR/pkgconfig/"
rm -f "/out/$LIBDIR"/*.a
