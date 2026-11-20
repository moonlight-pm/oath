# Vim, the editor (R18), also vi. Its runtime files are under this slot.
# `:version` names the builder's host, which the sandbox makes `build`.
# From 9.2.1255, runtime/syntax/shared/debversions.vim runs debian-distro-info
# and ubuntu-distro-info when they are on PATH, with fixed arguments; Oath ships
# neither (Joshua, 2026-11-18).
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
