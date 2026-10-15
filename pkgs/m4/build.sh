tar -xf "$SRC/$SOURCES"
cd m4-$VERSION
./configure --prefix=/ --disable-nls
make -j"$JOBS"
mkdir -p /out/bin
cp src/m4 /out/bin/m4
