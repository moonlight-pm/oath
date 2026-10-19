# tmux. Its default TERM is named, not probed from the build host's
# terminfo.
tar -xf "$SRC/$SOURCES"
cd "tmux-$VERSION"
./configure --prefix="$LIVE" --with-TERM=tmux-256color
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/bin" /out/
