tar -xf "$SRC/$SOURCES"
cd ninja-$VERSION
# BUILD_TESTING would fetch googletest.
cmake -S . -B build -DCMAKE_BUILD_TYPE=Release -DBUILD_TESTING=OFF
make -C build -j"$JOBS" ninja
mkdir -p /out/bin
cp build/ninja /out/bin/ninja
