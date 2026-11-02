# Output: bin/, lib/ (libpq), its modules in lib/postgresql, include/ and
# share/. The build puts modules in lib/ when the prefix has "postgres" in its
# path, as this one does, and /lib would link them all: pkglibdir says where.
tar -xf "$SRC/$SOURCES"
cd "postgresql-$VERSION"
./configure --prefix="$LIVE" --libdir="$LIVE/$LIBDIR" --with-openssl --with-icu --with-libxml \
	--with-readline --with-zstd --with-uuid=e2fs --disable-rpath
mods="pkglibdir=$LIVE/$LIBDIR/postgresql"
make -j"$JOBS" world-bin "$mods"
make install-world-bin DESTDIR=/tmp/dest "$mods"
cp -a "/tmp/dest$LIVE/." /out/
rm -f "/out/$LIBDIR"/*.a

# It runs. The sandbox is root, which the server refuses, so no server here.
export LD_LIBRARY_PATH="/out/$LIBDIR"
/out/bin/postgres --version | grep -q " $VERSION\$"
/out/bin/psql --version | grep -q " $VERSION\$"
/out/bin/pg_config --pkglibdir | grep -q "/$LIBDIR/postgresql\$"
