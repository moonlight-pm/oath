# Output: lib/postgis/ (the modules), share/extension/ (the control files
# and SQL), bin/ (shp2pgsql and its kin). PostgreSQL's build installs under its
# own prefix, so this copies what landed there into this pack, and turns
# "$libdir/postgis-3" into this pack's path, since that module is not in
# PostgreSQL's $libdir. The modules are not in lib/postgresql/: /lib links
# each entry of a pack's lib/ for one package, and that one is pkg:postgresql's.
tar -xf "$SRC/$SOURCES"
cd "postgis-$VERSION"
export LD_LIBRARY_PATH=/oath/store/pkg/postgresql/live/$LIBDIR
./configure --prefix="$LIVE" --libdir="$LIVE/$LIBDIR" \
	--with-pgconfig=/oath/store/pkg/postgresql/live/bin/pg_config \
	--with-geosconfig=/oath/store/pkg/geos/live/bin/geos-config \
	--with-projdir=/oath/store/pkg/proj/live \
	--with-xml2config=/oath/store/pkg/libxml2/live/bin/xml2-config \
	--with-jsondir=/oath/store/pkg/json-c/live \
	--without-raster --without-protobuf --without-address-standardizer --without-gui \
	--without-interrupt-tests --disable-extension-upgrades-install
make -j"$JOBS"
make install DESTDIR=/tmp/dest
# pg_config answers from where its binary really is, the pack's hash.
pgc=/oath/store/pkg/postgresql/live/bin/pg_config
mkdir -p "/out/$LIBDIR/postgis" /out/share/extension /out/bin
cp -a "/tmp/dest$($pgc --pkglibdir)"/postgis* "/out/$LIBDIR/postgis/"
cp -a "/tmp/dest$($pgc --sharedir)/extension"/postgis* /out/share/extension/
[ ! -d "/tmp/dest$LIVE/bin" ] || cp -a "/tmp/dest$LIVE/bin/." /out/bin/
# postgis and postgis_restore are Perl scripts that start #!/usr/bin/perl: Oath
# has no /usr, and Perl is not among what this pack runs with.
rm -f /out/bin/postgis /out/bin/postgis_restore
for f in /out/bin/*; do
	if head -n1 "$f" | grep -q '^#!/usr'; then
		echo "postgis: bin/${f##*/} starts $(head -n1 "$f"), and there is no /usr" >&2
		exit 1
	fi
done
find /tmp/dest -type f -name 'shp2pgsql' -o -type f -name 'pgtopo_export' | sed 's|/tmp/dest/oath/store/pkg/||'
grep -rl '\$libdir/postgis' /out/share/extension | while read -r f; do
	sed -i "s|\\\$libdir/postgis|$LIVE/$LIBDIR/postgis/postgis|g" "$f"
done
if grep -rq '\$libdir/' /out/share/extension; then
	echo 'postgis: an extension still names $libdir' >&2
	exit 1
fi
if grep -rq "$LIVE/$LIBDIR/postgresql" /out; then
	echo "postgis: a file still names $LIVE/$LIBDIR/postgresql" >&2
	exit 1
fi
for f in "/out/$LIBDIR"/*; do
	if [ -e "/oath/store/pkg/postgresql/live/$LIBDIR/${f##*/}" ]; then
		echo "postgis: $LIBDIR/${f##*/} is in pkg:postgresql too, and /lib links it for one" >&2
		exit 1
	fi
done
