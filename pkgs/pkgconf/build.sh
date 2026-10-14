tar -xf "$SRC/$SOURCES"
cd pkgconf-$VERSION
./configure --prefix=/ --disable-shared --enable-static
make -j"$JOBS"
mkdir -p /out/bin
cp pkgconf /out/bin/pkgconf
ln -s pkgconf /out/bin/pkg-config
