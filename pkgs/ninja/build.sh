# Ninja, by its own bootstrap (Python and the C++ compiler; no CMake).
tar -xf "$SRC/$SOURCES"
cd "ninja-$VERSION"
python3 configure.py --bootstrap
mkdir -p /out/bin
cp ninja /out/bin/ninja
