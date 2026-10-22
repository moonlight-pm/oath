# glslang and glslangValidator (a link to it), linked against glslang's
# own static libraries; the pack is the two programs.
tar -xf "$SRC/$SOURCES"
cd "glslang-$VERSION"
cmake -S . -B build -DCMAKE_INSTALL_PREFIX="$LIVE" -DCMAKE_BUILD_TYPE=Release \
	-DENABLE_OPT=OFF -DGLSLANG_TESTS=OFF -DBUILD_SHARED_LIBS=OFF
cmake --build build -j"$JOBS"
DESTDIR=/tmp/dest cmake --install build
mkdir -p /out/bin
cp -a "/tmp/dest$LIVE/bin/glslang" "/tmp/dest$LIVE/bin/glslangValidator" /out/bin/
