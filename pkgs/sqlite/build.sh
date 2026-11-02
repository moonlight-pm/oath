# Output: lib/libsqlite3.a, position-independent so a shared library can link
# it, include/sqlite3.h, the pkg-config file and bin/sqlite3.
tar -xf "$SRC/$SOURCES"
cd "sqlite-version-$VERSION"
CFLAGS="$CFLAGS -fPIC" ./configure --prefix="$LIVE" --libdir="$LIVE/$LIBDIR" \
	--disable-shared --enable-static --rtree --fts5 --disable-tcl --disable-readline
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/." /out/
rm -rf /out/share/man
[ "$(/out/bin/sqlite3 :memory: 'select sqlite_version()')" = "$VERSION" ]
/out/bin/sqlite3 :memory: "create virtual table t using fts5(a); create virtual table r using rtree(i, x, y)"
