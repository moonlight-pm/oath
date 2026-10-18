# libxshmfence, shared, on futexes. memfd comes first at run time; the
# directory is the fallback, named because an empty sandbox has none to
# find.
tar -xf "$SRC/$SOURCES"
cd "libxshmfence-$VERSION"
./configure --prefix="$LIVE" --disable-static --enable-futex --with-shared-memory-dir=/dev/shm
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/lib" /out/
rm -f /out/lib/*.la
