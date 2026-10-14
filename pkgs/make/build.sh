tar -xf "$SRC/$SOURCES"
cd make-$VERSION
./configure --prefix=/ --disable-nls --without-guile CC=cc AR=ar RANLIB=ranlib
make -j"$JOBS"
mkdir -p /out/bin
cp make /out/bin/make
ln -s make /out/bin/gmake
