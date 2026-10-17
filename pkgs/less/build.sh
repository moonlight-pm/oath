# less, the pager.
tar -xf "$SRC/$SOURCES"
cd "less-$VERSION"
./configure --prefix="$LIVE" --sysconfdir=/etc --with-regex=posix
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/bin" /out/
