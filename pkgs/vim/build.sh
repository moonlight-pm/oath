# Vim, the editor (R18), also vi. Its runtime files are under this slot.
# `:version` names the builder's host, which the sandbox makes `build`.
# patches/: upstream's fixes of GHSA-p99j-mfjp-r4cg (9.2.1308, the socket
# server's directory) and GHSA-26qr-jvfj-hcwm (9.2.1309, spell files), which a
# pin of those tags would bring with a new process (debversions.vim, 9.2.1255).
tar -xf "$SRC/$SOURCES"
cd "vim-$VERSION"
for p in "$FILES"/patches/*.patch; do
	patch -p1 -i "$p"
done
./configure --prefix="$LIVE" --with-features=huge --enable-multibyte --disable-gui \
	--without-x --disable-nls --with-tlib=ncursesw --disable-canberra --disable-libsodium \
	--disable-gpm --disable-sysmouse --disable-netbeans
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/bin" "/tmp/dest$LIVE/share" /out/
ln -sf vim /out/bin/vi
rm -rf /out/share/man /out/share/applications /out/share/icons
