# GNU findutils: find, xargs, locate, updatedb. The fstatat probe is
# pinned as in coreutils, so every builder makes the same find.
tar -xf "$SRC/$SOURCES"
cd "findutils-$VERSION"
gl_cv_func_fstatat_null_file=no ./configure --prefix="$LIVE" --disable-nls --localstatedir=/var/lib/locate
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/bin" "/tmp/dest$LIVE/libexec" /out/
