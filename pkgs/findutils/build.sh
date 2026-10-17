# GNU findutils: find, xargs, locate, updatedb.
tar -xf "$SRC/$SOURCES"
cd "findutils-$VERSION"
./configure --prefix="$LIVE" --disable-nls --localstatedir=/var/lib/locate
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/bin" "/tmp/dest$LIVE/libexec" /out/
