# libasound and share/alsa (alsa.conf, read from this slot at run time);
# no aserver (GPL), Python, topology or mixer modules. patches/: upstream
# fixes no release has yet, CVE-2026-90781 (control-name parser overrun)
# and CVE-2026-96675 (multi PCM binding checks).
tar -xf "$SRC/$SOURCES"
cd "alsa-lib-$VERSION"
for p in "$FILES"/patches/*.patch; do
	patch -p1 -i "$p"
done
./configure --prefix="$LIVE" --disable-static --disable-python --disable-aload \
	--disable-topology --disable-mixer-modules --without-debug
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/lib" "/tmp/dest$LIVE/share" /out/
rm -f /out/lib/*.la
