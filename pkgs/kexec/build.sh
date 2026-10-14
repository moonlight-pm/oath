tar -xf "$SRC/$SOURCES"
cd kexec-tools-$VERSION
# No separate assembler: clang assembles .S itself.
mkdir -p /tmp/bin
printf '#!/bin/sh\nexec cc -c "$@"\n' > /tmp/bin/as
chmod +x /tmp/bin/as
PATH=/tmp/bin:$PATH
./configure --prefix=/ --without-lzma --without-xen AS=as
make -j"$JOBS"
mkdir -p /out/bin
cp build/sbin/kexec /out/bin/kexec
