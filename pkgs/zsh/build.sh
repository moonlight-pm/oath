# zsh, home's login shell (R27). Its modules and functions are under this
# slot; the global startup files are /etc's, written by oath from
# host:local. No man pages or info.
tar -xf "$SRC/$SOURCES"
cd "zsh-$VERSION"
./configure --prefix="$LIVE" --enable-etcdir=/etc
make -j"$JOBS"
make install.bin install.modules install.fns DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/bin" "/tmp/dest$LIVE/lib" "/tmp/dest$LIVE/share" /out/
rm -f "/out/bin/zsh-$VERSION"
# _darcs is its one GPL file, and LICENCE lets such files be left out.
rm -f "/out/share/zsh/$VERSION/functions/_darcs"
# No zsh-newuser-install menu in a new account's first shell.
rm -f "/out/share/zsh/$VERSION/scripts/newuser"
