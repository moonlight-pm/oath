# kexec, the installer's way into its ramdisk. It runs on the machine
# being replaced, whatever its libc or loader path, so it is static (as
# Oath's own binaries are) and reads only uncompressed kernels.
tar -xf "$SRC/$SOURCES"
cd "kexec-tools-$VERSION"
./configure --prefix=/ --without-lzma --without-zlib --without-xen LDFLAGS=-static
make -j"$JOBS"
mkdir -p /out/bin
cp build/sbin/kexec /out/bin/kexec
if readelf -lW /out/bin/kexec | grep -q INTERP; then
	echo "kexec is not static" >&2
	exit 1
fi
