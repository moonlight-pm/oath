# GNU awk; its extensions are in lib/gawk and its awk library in share/awk.
tar -xf "$SRC/$SOURCES"
cd "gawk-$VERSION"
sed -i 's/extras//' Makefile.in
./configure --prefix="$LIVE" --disable-nls
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/bin" "/tmp/dest$LIVE/lib" "/tmp/dest$LIVE/libexec" /out/
mkdir -p /out/share
cp -a "/tmp/dest$LIVE/share/awk" /out/share/
