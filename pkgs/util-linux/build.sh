tar -xf "$SRC/$SOURCES"
cd util-linux-$VERSION
./configure --prefix="$LIVE" --disable-shared --enable-static \
	--disable-all-programs --enable-libuuid --enable-libblkid --enable-libfdisk \
	--enable-libsmartcols --enable-libmount --enable-fdisks=check --enable-wipefs \
	--enable-blkid --enable-lsblk \
	--disable-nls --disable-bash-completion --disable-asciidoc --disable-poman \
	--without-python --without-systemd --without-udev --without-ncurses \
	--without-ncursesw --without-tinfo --without-readline --without-libz \
	--without-btrfs --without-cap-ng --without-econf --without-selinux \
	--without-audit --without-utempter --without-user
make -j"$JOBS"
make install DESTDIR=/tmp/dest
mkdir -p /out/bin
cp -a "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/lib" /out/
for b in sfdisk wipefs blkid lsblk; do
	cp "$(find "/tmp/dest$LIVE" -name "$b" -type f | head -1)" "/out/bin/$b"
done
