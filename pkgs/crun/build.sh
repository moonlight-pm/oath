# Output: bin/crun.
tar -xf "$SRC/$SOURCES"
cd "crun-$VERSION"
./configure --prefix="$LIVE" --libdir="$LIVE/$LIBDIR" --disable-systemd --disable-criu \
	--disable-static
make -j"$JOBS"
make install DESTDIR=/tmp/dest
mkdir -p /out/bin
cp -a "/tmp/dest$LIVE/bin/crun" /out/bin/
/out/bin/crun --version | grep -q "^crun version $VERSION"
/out/bin/crun --version | grep -q '+SECCOMP'
