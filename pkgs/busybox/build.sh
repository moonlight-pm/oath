# busybox from pkgs/busybox/config (defconfig, static, no init/tc).
tar -xf "$SRC/$SOURCES"
cd busybox-$VERSION
cp "$FILES/config" .config
make HOSTCC=cc CC=cc oldconfig </dev/null >/dev/null
make HOSTCC=cc CC=cc AR=ar STRIP=strip -j"$JOBS" busybox
mkdir -p /out/bin
cp busybox /out/bin/busybox
for a in $(./busybox --list); do
	[ "$a" = busybox ] || ln -s busybox "/out/bin/$a"
done
