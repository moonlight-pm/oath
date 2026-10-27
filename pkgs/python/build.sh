# CPython, with its stdlib extensions as shared modules (lib-dynload):
# ssl and hashlib against pkg:openssl (which trusts curl's CA bundle),
# ctypes against pkg:libffi. Not built: those whose libraries Oath does
# not have (SQLite, Tk). It finds its stdlib by its own path.
tar -xf "$SRC/$SOURCES"
cd "Python-$VERSION"
./configure --prefix=/ --without-ensurepip --disable-test-modules
make -j"$JOBS"
make install DESTDIR=/out
# CPython compiles with -g, and its DWARF names the compiler's store
# path: drop it, or every file changes with every pkg:gcc.
strip --strip-debug "/out/bin/python$(echo "$VERSION" | cut -d. -f1,2)"
find /out/lib -name '*.so' -exec strip --strip-debug {} +
rm -rf /out/share /out/lib/libpython*.a /out/lib/python3*/config-*/libpython*.a \
	/out/lib/python3*/idlelib /out/lib/python3*/tkinter \
	/out/lib/python3*/turtledemo /out/bin/idle3*
