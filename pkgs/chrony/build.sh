# chrony: chronyd keeps the clock (svc:time, R12, R18). It takes its
# directives on the command line; it has no command socket (no chronyc).
# After start-up it drops to void (R36), keeping only the capability to
# set the clock (libcap).
tar -xf "$SRC/$SOURCES"
cd "chrony-$VERSION"
./configure --prefix="$LIVE" --sbindir="$LIVE/bin" --sysconfdir=/etc --localstatedir=/var \
	--with-pidfile=/run/chronyd.pid --with-user=void --disable-cmdmon --disable-nts \
	--disable-scfilter --disable-readline --without-editline \
	--without-seccomp --without-nettle --without-nss --without-tomcrypt --without-gnutls
make -j"$JOBS"
mkdir -p /out/bin
cp chronyd /out/bin/
