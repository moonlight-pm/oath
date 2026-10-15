tar -xf "$SRC/$SOURCES"
cd cmake-$VERSION
# libarchive looks for iconv.h with find_path, which does not know the
# compiler's sysroot (musl has iconv in libc).
sysinc=$(cd "$(cc -print-resource-dir)/../../../sysroot/include" && pwd)
# The bundled curl has no TLS: nothing downloads inside a sandbox. No
# curses or Qt front end.
./bootstrap --prefix=/ --parallel="$JOBS" --no-system-libs --no-qt-gui -- \
	-DCMAKE_BUILD_TYPE=Release -DBUILD_TESTING=OFF \
	-DBUILD_CursesDialog=OFF -DCMAKE_USE_OPENSSL=OFF \
	-DICONV_INCLUDE_DIR="$sysinc"
make -j"$JOBS"
make install DESTDIR=/out
rm -rf /out/doc /out/share/man /out/share/aclocal /out/share/bash-completion \
	/out/share/emacs /out/share/vim
