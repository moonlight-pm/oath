# btrfs and mkfs.btrfs, against the shared zlib, zstd and util-linux's libblkid
# and libuuid. No LZO, whose project has no public history to check: `btrfs
# restore` cannot read LZO extents, and the kernel still can.
tar -xf "$SRC/$SOURCES"
cd "btrfs-progs-v$VERSION"
./configure --prefix=/ --disable-documentation --disable-python --disable-convert \
	--disable-lzo --disable-libudev --disable-shared --disable-zoned --disable-backtrace
make -j"$JOBS" btrfs mkfs.btrfs
mkdir -p /out/bin
cp btrfs mkfs.btrfs /out/bin/
