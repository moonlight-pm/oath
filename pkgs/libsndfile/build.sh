# libsndfile, shared, with headers and pkg-config; WAV, AIFF, AU, CAF and
# the other PCM formats. FLAC, Ogg, Vorbis and Opus come all together or
# not at all, so not. 1.2.2 predates C23 (ALAC defines its own bool), so gnu17.
# patches/: fixes from upstream's master, which no release has yet: the
# IMA ADPCM frame-count overflow (3d63704, CVE-2026-37555 by its
# description), MAT4 channel counts (CVE-2026-88372) and WAV cue names
# (CVE-2026-109828), the last two ported to 1.2.2.
tar -xf "$SRC/$SOURCES"
cd "libsndfile-$VERSION"
for p in "$FILES"/patches/*.patch; do
	patch -p1 -i "$p"
done
CFLAGS="$CFLAGS -std=gnu17" ./configure --prefix="$LIVE" --disable-static --disable-external-libs --disable-mpeg \
	--disable-sqlite --disable-alsa --disable-full-suite
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/lib" /out/
rm -f /out/lib/*.la
