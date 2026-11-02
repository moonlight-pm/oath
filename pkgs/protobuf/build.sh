# Output: bin/protoc and include/google/protobuf/*.proto, the well-known
# types. Abseil and protobuf's libraries are linked into protoc, not shipped.
tar -xf "$SRC/protobuf-$VERSION.tar.gz"
tar -xf "$SRC"/abseil-cpp-*.tar.gz
mv abseil-cpp-* abseil-cpp
cd "protobuf-$VERSION"
cmake -S . -B build -G Ninja -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="$LIVE" \
	-DCMAKE_INSTALL_LIBDIR="$LIBDIR" -DBUILD_SHARED_LIBS=OFF -Dprotobuf_BUILD_SHARED_LIBS=OFF \
	-Dprotobuf_BUILD_TESTS=OFF -Dprotobuf_BUILD_LIBUPB=OFF -Dprotobuf_FORCE_FETCH_DEPENDENCIES=ON \
	-DFETCHCONTENT_FULLY_DISCONNECTED=ON -DFETCHCONTENT_SOURCE_DIR_ABSL="$(cd ..; pwd)/abseil-cpp"
cmake --build build -j"$JOBS" --target protoc
mkdir -p /out/bin /out/include/google/protobuf /out/share/doc/protobuf
cp build/protoc /out/bin/
cp src/google/protobuf/any.proto src/google/protobuf/api.proto src/google/protobuf/descriptor.proto \
	src/google/protobuf/duration.proto src/google/protobuf/empty.proto \
	src/google/protobuf/field_mask.proto src/google/protobuf/source_context.proto \
	src/google/protobuf/struct.proto src/google/protobuf/timestamp.proto \
	src/google/protobuf/type.proto src/google/protobuf/wrappers.proto /out/include/google/protobuf/
cp LICENSE /out/share/doc/protobuf/

/out/bin/protoc --version | grep -q "libprotoc $VERSION"
mkdir /tmp/pb
echo 'syntax = "proto3"; message M { string s = 1; }' >/tmp/pb/m.proto
/out/bin/protoc -I/tmp/pb --cpp_out=/tmp/pb /tmp/pb/m.proto
[ -s /tmp/pb/m.pb.h ]
