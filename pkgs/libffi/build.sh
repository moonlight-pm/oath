# libffi, shared, with headers and pkg-config. Not tuned to the build
# host's CPU, and in lib/, not lib64/.
tar -xf "$SRC/$SOURCES"
cd "libffi-$VERSION"
./configure --prefix="$LIVE" --disable-static --disable-docs --disable-multi-os-directory \
	--without-gcc-arch
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/lib" /out/
rm -f /out/lib/*.la
