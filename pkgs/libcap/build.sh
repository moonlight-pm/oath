# libcap.so with its header and pkg-config; not the tools, libpsx, the
# PAM module or the Go packages. Serial: with -j its objects can build
# before cap_names.h is generated.
tar -xf "$SRC/$SOURCES"
cd "libcap-$VERSION"
make -C libcap prefix="$LIVE" lib=lib GOLANG=no PAM_CAP=no USE_GPERF=no \
	FAKEROOT=/tmp/dest install-shared-cap
cp -a "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/lib" /out/
