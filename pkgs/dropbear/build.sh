# dropbear (the server, svc:sshd) and dropbearkey, against the shared zlib.
# patches/session-env.patch: sessions keep the server's environment
# (host:local env from PID 1), less its XDG_RUNTIME_DIR, so `ssh host cmd`
# sees what a login shell gets from /etc/profile.
tar -xf "$SRC/$SOURCES"
cd "dropbear-$VERSION"
for p in "$FILES"/patches/*.patch; do
	echo "patch ${p##*/}"
	patch -p1 -i "$p"
done
cp "$FILES/localoptions.h" .
./configure --prefix=/ --disable-lastlog --disable-utmp --disable-utmpx --disable-wtmp \
	--disable-wtmpx --disable-pututline --disable-pututxline
make -j"$JOBS" PROGRAMS="dropbear dropbearkey"
mkdir -p /out/bin
cp dropbear dropbearkey /out/bin/
