# configure insists on argp, fts and obstack for the tools and libdw;
# libelf uses none of them, so tell it they are there and build only
# libelf/, with the libeu helpers it folds in. Left out: color.c and
# printversion.c (for the tools; they include argp.h) and crc32.c and
# crc32_file.c (libelf has its own; a public crc32 clashes with zlib's
# in a static link).
tar -xf "$SRC/$SOURCES"
cd elfutils-$VERSION
./configure --prefix="$LIVE" --disable-nls --disable-debuginfod \
	--disable-libdebuginfod --disable-demangler --disable-symbol-versioning \
	--without-bzlib --without-lzma --without-zstd \
	ac_cv_search_argp_parse='none required' \
	ac_cv_search_fts_close='none required' \
	ac_cv_search__obstack_free='none required'
eu="xasprintf.o xstrdup.o xstrndup.o xmalloc.o next_prime.o eu-search.o error.o"
# shellcheck disable=SC2086
make -C lib -j"$JOBS" $eu
echo "$eu" >lib/libeu.manifest
make -C libelf -j"$JOBS" libelf.a
mkdir -p /out/include /out/lib/pkgconfig
cp libelf/libelf.h libelf/gelf.h libelf/nlist.h /out/include/
cp libelf/libelf.a /out/lib/
cp config/libelf.pc /out/lib/pkgconfig/
