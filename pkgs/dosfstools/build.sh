tar -xf "$SRC/$SOURCES"
cd dosfstools-$VERSION
./configure --prefix=/ --without-udev
make -j"$JOBS"
mkdir -p /out/bin
cp src/mkfs.fat src/fsck.fat /out/bin/
ln -s mkfs.fat /out/bin/mkfs.vfat
