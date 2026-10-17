# GNU bc and dc (the kernel computes timeconst.h with bc).
tar -xf "$SRC/$SOURCES"
cd "bc-$VERSION"
./configure --prefix="$LIVE"
make -j"$JOBS"
mkdir -p /out/bin
cp bc/bc dc/dc /out/bin/
