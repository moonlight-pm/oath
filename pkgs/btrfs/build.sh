tar -xf "$SRC/$SOURCES"
cd btrfs-progs-v$VERSION
./configure --prefix=/ --disable-documentation --disable-python --disable-convert \
	--disable-libudev --disable-shared --disable-zoned --disable-backtrace
make -j"$JOBS" btrfs mkfs.btrfs
mkdir -p /out/bin
cp btrfs mkfs.btrfs /out/bin/
