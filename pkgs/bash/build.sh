# bash 5.3 with its upstream patches (bash53-001…), also /bin/sh (R18).
tar -xf "$SRC/bash-$VERSION.tar.gz"
cd "bash-$VERSION"
for p in "$SRC"/bash53-*; do patch -p0 -i "$p"; done
./configure --prefix="$LIVE" --without-bash-malloc --disable-nls
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/bin" /out/
ln -s bash /out/bin/sh
