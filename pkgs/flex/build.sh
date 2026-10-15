# The release ships its own scanner (src/scan.c), so flex is not needed
# to build flex. It runs m4 from PATH (the path configure found).
tar -xf "$SRC/$SOURCES"
cd flex-$VERSION
./configure --prefix=/ --disable-nls --disable-shared --disable-bootstrap
make -C src -j"$JOBS" flex
mkdir -p /out/bin
cp src/flex /out/bin/flex
ln -s flex /out/bin/lex
