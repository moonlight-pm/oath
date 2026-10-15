# Bison reads its skeletons from share/bison under the slot path and runs
# m4 from PATH (the path configure found).
tar -xf "$SRC/$SOURCES"
cd bison-$VERSION
./configure --prefix="$LIVE" --disable-nls
make -j"$JOBS"
make install DESTDIR=/tmp/dest
mkdir -p /out/share
cp -a "/tmp/dest$LIVE/bin" /out/
cp -a "/tmp/dest$LIVE/share/bison" /out/share/
