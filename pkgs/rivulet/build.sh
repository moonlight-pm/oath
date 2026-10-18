# rivulet and River's protocol XML (share/river-protocols), which window
# managers such as sola-river build against.
cp -a "$SRC/rivulet" /tmp/rivulet
cd /tmp/rivulet
make -j"$JOBS" PREFIX="$LIVE"
make install PREFIX="$LIVE" DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/bin" "/tmp/dest$LIVE/share" /out/
