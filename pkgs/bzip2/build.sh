# bzip2 and libbz2 (shared). It has no configure: its two Makefiles.
# patches/: bzip2recover's block-count fix (CVE-2026-42250), from
# upstream git; no release has it.
tar -xf "$SRC/$SOURCES"
cd "bzip2-$VERSION"
for p in "$FILES"/patches/*.patch; do
	patch -p1 -i "$p"
done
make -f Makefile-libbz2_so CC="$CC" CFLAGS="$CFLAGS -fPIC -D_FILE_OFFSET_BITS=64"
make clean
make CC="$CC" CFLAGS="$CFLAGS -D_FILE_OFFSET_BITS=64" libbz2.a bzip2 bzip2recover
mkdir -p /out/bin /out/lib /out/include
cp bzip2-shared /out/bin/bzip2
cp bzip2recover /out/bin/
ln -s bzip2 /out/bin/bunzip2
ln -s bzip2 /out/bin/bzcat
cp -a "libbz2.so.$VERSION" /out/lib/
ln -s "libbz2.so.$VERSION" /out/lib/libbz2.so.1.0
ln -s "libbz2.so.$VERSION" /out/lib/libbz2.so
cp libbz2.a /out/lib/
cp bzlib.h /out/include/
