# share/hwdata/pnp.ids, and hwdata.pc in lib/pkgconfig, where the
# sandbox looks.
tar -xf "$SRC/$SOURCES"
cd "hwdata-$VERSION"
install -Dm644 pnp.ids /out/share/hwdata/pnp.ids
mkdir -p /out/lib/pkgconfig
sed -e "s|@prefix@|$LIVE|" -e "s|@datadir@|$LIVE/share|" -e "s|@pkgdatadir@|$LIVE/share/hwdata|" \
	-e "s|@NAME@|hwdata|" -e "s|@VERSION@|$VERSION|" hwdata.pc.in >/out/lib/pkgconfig/hwdata.pc
