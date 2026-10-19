# libasound and share/alsa (alsa.conf, read from this slot at run time);
# no aserver (GPL), Python, topology or mixer modules.
tar -xf "$SRC/$SOURCES"
cd "alsa-lib-$VERSION"
./configure --prefix="$LIVE" --disable-static --disable-python --disable-aload \
	--disable-topology --disable-mixer-modules --without-debug
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/lib" "/tmp/dest$LIVE/share" /out/
rm -f /out/lib/*.la
