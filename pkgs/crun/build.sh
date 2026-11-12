# Output: bin/crun. patches/: Oath's, which keeps a terminal's line endings.
tar -xf "$SRC/$SOURCES"
cd "crun-$VERSION"
for p in "$FILES"/patches/*.patch; do
	echo "patch ${p##*/}"
	patch -p1 -i "$p"
done
./configure --prefix="$LIVE" --libdir="$LIVE/$LIBDIR" --disable-systemd --disable-criu \
	--disable-static
make -j"$JOBS"
make install DESTDIR=/tmp/dest
mkdir -p /out/bin
cp -a "/tmp/dest$LIVE/bin/crun" /out/bin/
/out/bin/crun --version | grep -q "^crun version $VERSION"
/out/bin/crun --version | grep -q '+SECCOMP'
