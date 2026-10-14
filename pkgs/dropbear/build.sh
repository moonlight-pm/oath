tar -xf "$SRC/$SOURCES"
cd dropbear-$VERSION
cp "$FILES/localoptions.h" .
./configure --prefix=/ --enable-static --disable-lastlog --disable-utmp \
	--disable-utmpx --disable-wtmp --disable-wtmpx --disable-pututline \
	--disable-pututxline
make -j"$JOBS" PROGRAMS="dropbear dropbearkey"
mkdir -p /out/bin
cp dropbear dropbearkey /out/bin/
