# ncurses (wide characters) and the terminfo database, which programs
# find at its compiled-in path under this slot. The narrow names link to
# the wide libraries.
tar -xf "$SRC/$SOURCES"
cd "ncurses-$VERSION"
./configure --prefix="$LIVE" --with-shared --without-normal --without-debug --without-ada \
	--without-cxx-binding --without-tests --without-manpages --enable-widec --enable-pc-files \
	--with-pkg-config-libdir="$LIVE/lib/pkgconfig" --disable-stripping
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/bin" "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/lib" "/tmp/dest$LIVE/share" /out/
for l in ncurses form panel menu; do
	ln -sf "lib${l}w.so" "/out/lib/lib$l.so"
	ln -sf "${l}w.pc" "/out/lib/pkgconfig/$l.pc"
done
ln -sf libncursesw.so /out/lib/libcurses.so
rm -rf /out/share/man /out/share/doc
