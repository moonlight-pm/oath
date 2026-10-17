# util-linux: mount, umount, dmesg, fdisk, sfdisk, wipefs, blkid, lsblk,
# findmnt, losetup, blockdev and the rest, with libblkid, libuuid,
# libmount, libfdisk and libsmartcols. Left out: what Oath or another
# package does (logins and su, kill, more) and what needs utmp.
tar -xf "$SRC/$SOURCES"
cd "util-linux-$VERSION"
./configure --prefix="$LIVE" --disable-static \
	--disable-login --disable-su --disable-sulogin --disable-nologin --disable-runuser \
	--disable-agetty --disable-chfn-chsh --disable-newgrp --disable-vipw --disable-wall \
	--disable-write --disable-mesg --disable-more --disable-kill --disable-last \
	--disable-utmpdump --disable-lslogins --disable-uuidd --disable-liblastlog2 \
	--disable-pam-lastlog2 --disable-makeinstall-chown --disable-makeinstall-setuid \
	--disable-nls --disable-bash-completion --disable-asciidoc --disable-poman \
	--without-python --without-systemd --without-udev --without-ncurses \
	--without-ncursesw --without-tinfo --without-readline --without-btrfs \
	--without-cap-ng --without-econf --without-selinux --without-audit \
	--without-utempter --without-user
make -j"$JOBS"
make install DESTDIR=/tmp/dest
mkdir -p /out/bin
cp -a "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/lib" /out/
for d in bin sbin; do
	[ -d "/tmp/dest$LIVE/$d" ] && cp -a "/tmp/dest$LIVE/$d/." /out/bin/
done
find /out/lib -name '*.la' -delete
