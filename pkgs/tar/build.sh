# GNU tar.
tar -xf "$SRC/$SOURCES"
cd "tar-$VERSION"
FORCE_UNSAFE_CONFIGURE=1 ./configure --prefix="$LIVE" --disable-nls
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/bin" "/tmp/dest$LIVE/libexec" /out/
