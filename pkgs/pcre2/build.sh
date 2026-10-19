# libpcre2-8, shared, with JIT; no pcre2grep or pcre2test.
tar -xf "$SRC/$SOURCES"
cd "pcre2-$VERSION"
./configure --prefix="$LIVE" --disable-static --enable-jit
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/lib" /out/
rm -f /out/lib/*.la
