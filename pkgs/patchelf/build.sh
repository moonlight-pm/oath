tar -xf "$SRC/$SOURCES"
cd patchelf-$VERSION
./configure --prefix=/
make -j"$JOBS"
mkdir -p /out/bin
cp src/patchelf /out/bin/patchelf
