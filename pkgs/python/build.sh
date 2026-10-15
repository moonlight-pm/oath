tar -xf "$SRC/$SOURCES"
cd Python-$VERSION
# One static binary: a static musl program cannot dlopen, so every
# stdlib extension is linked in (MODULE_BUILDTYPE=static).
MODULE_BUILDTYPE=static ./configure --prefix=/ --disable-shared \
	--without-ensurepip --disable-test-modules
make -j"$JOBS"
make install DESTDIR=/out
# The interpreter is the product: nothing can embed or extend a static
# python, so libpython (installed twice) goes, and so do IDLE and
# tkinter (_tkinter is not built).
rm -rf /out/share /out/lib/libpython*.a /out/lib/python3*/config-*/libpython*.a \
	/out/lib/python3*/idlelib /out/lib/python3*/tkinter \
	/out/lib/python3*/turtledemo /out/bin/idle3*
