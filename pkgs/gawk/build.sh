# GNU awk, with MPFR (gawk 5.4.1 without it miscompiles the awk scripts
# that generate GCC 16.2's option tables). Its extensions are in lib/gawk
# and its awk library in share/awk.
tar -xf "$SRC/$SOURCES"
cd "gawk-$VERSION"
sed -i 's/extras//' Makefile.in
./configure --prefix="$LIVE" --disable-nls
make -j"$JOBS"
./gawk --version | grep -q 'GNU MPFR' || { echo "gawk was built without MPFR" >&2; exit 1; }
make install DESTDIR=/tmp/dest
cp -a "/tmp/dest$LIVE/bin" "/tmp/dest$LIVE/lib" "/tmp/dest$LIVE/libexec" /out/
mkdir -p /out/share
cp -a "/tmp/dest$LIVE/share/awk" /out/share/
