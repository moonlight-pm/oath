# libsndfile, shared, with headers and pkg-config; WAV, AIFF, AU, CAF and
# the other PCM formats. FLAC, Ogg, Vorbis and Opus come all together or
# not at all, so not. 1.2.2 predates C23 (ALAC defines its own bool), so gnu17.
tar -xf "$SRC/$SOURCES"
cd "libsndfile-$VERSION"
CFLAGS="$CFLAGS -std=gnu17" ./configure --prefix="$LIVE" --disable-static --disable-external-libs --disable-mpeg \
	--disable-sqlite --disable-alsa --disable-full-suite
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/lib" /out/
rm -f /out/lib/*.la
