# GNU coreutils, with hostname; kill and uptime are procps-ng's.
tar -xf "$SRC/$SOURCES"
cd "coreutils-$VERSION"
FORCE_UNSAFE_CONFIGURE=1 ./configure --prefix="$LIVE" --disable-nls \
	--enable-install-program=hostname --enable-no-install-program=kill,uptime
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/bin" "/tmp/dest$LIVE/libexec" /out/
