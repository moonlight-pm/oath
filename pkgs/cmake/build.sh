# CMake, bootstrapped with gcc and make; its own copies of curl,
# libarchive and the rest, no OpenSSL, no curses dialog, no docs.
# glibc's headers are in its slot, not /usr/include: libarchive is told
# where iconv.h is.
tar -xf "$SRC/$SOURCES"
cd "cmake-$VERSION"
./bootstrap --prefix="$LIVE" --parallel="$JOBS" --no-qt-gui --no-system-libs -- \
	-DCMAKE_USE_OPENSSL=OFF -DBUILD_TESTING=OFF -DBUILD_CursesDialog=OFF -DCMAKE_BUILD_TYPE=Release \
	-DICONV_INCLUDE_DIR=/oath/store/pkg/glibc/live/include
make -j"$JOBS"
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/bin" "/tmp/dest$LIVE/share" /out/
rm -rf /out/share/doc /out/share/bash-completion /out/share/emacs /out/share/vim
