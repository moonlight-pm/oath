# Output: lib/libsqlite3.a, position-independent so a shared library can link
# it, include/sqlite3.h, the pkg-config file and bin/sqlite3.
tar -xf "$SRC/$SOURCES"
cd sqlite-autoconf-3540100
CFLAGS="$CFLAGS -fPIC" ./configure --prefix="$LIVE" --libdir="$LIVE/$LIBDIR" \
	--disable-shared --enable-static --enable-rtree --enable-fts5 --disable-readline
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/." /out/
rm -rf /out/share/man
[ "$(/out/bin/sqlite3 :memory: 'select sqlite_version()')" = "$VERSION" ]
