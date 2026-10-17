# Vim, the editor (R18), also vi. Its runtime files are under this slot.
tar -xf "$SRC/$SOURCES"
cd "vim-$VERSION"
./configure --prefix="$LIVE" --with-features=huge --enable-multibyte --disable-gui \
	--without-x --disable-nls --with-tlib=ncursesw --disable-canberra --disable-libsodium \
	--disable-gpm --disable-sysmouse --disable-netbeans
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/bin" "/tmp/dest$LIVE/share" /out/
ln -sf vim /out/bin/vi
rm -rf /out/share/man /out/share/applications /out/share/icons
