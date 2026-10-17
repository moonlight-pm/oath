# GNU make.
tar -xf "$SRC/$SOURCES"
cd "make-$VERSION"
./configure --prefix="$LIVE" --disable-nls --without-guile
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/bin" /out/
ln -s make /out/bin/gmake
