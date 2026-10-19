# Userspace RCU, every flavour, shared, with headers and pkg-config; no
# docs or examples.
tar -xf "$SRC/$SOURCES"
cd "userspace-rcu-$VERSION"
./configure --prefix="$LIVE" --disable-static
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/lib" /out/
rm -f /out/lib/*.la
