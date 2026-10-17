# GNU grep (no PCRE: -P is not built).
tar -xf "$SRC/$SOURCES"
cd "grep-$VERSION"
./configure --prefix="$LIVE" --disable-nls --disable-perl-regexp
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/bin" /out/
