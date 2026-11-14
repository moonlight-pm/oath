# ffmpeg and ffprobe, and libavcodec and the rest, shared, with headers and
# pkg-config. --disable-autodetect keeps out whatever else the sandbox has:
# no ffplay (SDL), no VA-API, VDPAU, NVENC or Vulkan, no outside codec.
# patches/: fixes from FFmpeg's master, which no release has yet, one file to
# an upstream pull request (or to a file they share), named for the CVEs.
tar -xf "$SRC/$SOURCES"
cd "ffmpeg-$VERSION"
for p in "$FILES"/patches/*.patch; do
	patch -p1 -i "$p"
done
./configure --prefix="$LIVE" --libdir="$LIVE/$LIBDIR" --shlibdir="$LIVE/$LIBDIR" \
	--enable-shared --disable-static --disable-autodetect --disable-doc --disable-debug \
	--enable-openssl --enable-zlib --enable-bzlib --enable-lzma --enable-iconv
grep -qx '#define HAVE_X86ASM 1' config.h
grep -qx '#define CONFIG_GPL 0' config.h
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/bin" "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/$LIBDIR" /out/
mkdir -p /out/share/doc/ffmpeg
cp LICENSE.md COPYING.LGPLv2.1 /out/share/doc/ffmpeg/

# It encodes, decodes and probes, and has https.
export LD_LIBRARY_PATH="/out/$LIBDIR"
cd /tmp
/out/bin/ffmpeg -hide_banner -loglevel error -f lavfi -i testsrc=duration=1:size=320x240:rate=10 \
	-f lavfi -i sine=duration=1 -c:v mjpeg -c:a aac -shortest t.mkv
/out/bin/ffprobe -v error -show_entries stream=codec_name -of csv=p=0 t.mkv | sort | paste -sd, | grep -qx aac,mjpeg
/out/bin/ffmpeg -hide_banner -loglevel error -i t.mkv -f null -
/out/bin/ffmpeg -hide_banner -protocols | grep -qx '  https'
