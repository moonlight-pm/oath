# fontconfig, shared, with fc-cache, fc-list, fc-match and fc-query. Its
# fonts.conf lists each font package's share/fonts, and
# 50-oath-families.conf maps sans-serif, serif and monospace to them
# (Inter has no serif companion yet). The cache is
# ~/.cache/fontconfig (the system one would be in the read-only store).
# The sandbox has no /usr/bin/env: Python scripts that are not
# executable run through meson's python.
tar -xf "$SRC/$SOURCES"
cd "fontconfig-$VERSION"
find . -name '*.py' -exec chmod -x {} +
fonts=/oath/store/pkg/font-inter/live/share/fonts,/oath/store/pkg/font-iosevka-term-slab/live/share/fonts
meson setup build --prefix="$LIVE" --libdir=lib --buildtype=release --wrap-mode=nodownload \
	-Ddoc=disabled -Dnls=disabled -Dtests=disabled -Dtools=enabled -Dcache-build=disabled \
	-Dxml-backend=expat -Diconv=disabled -Dfontations=disabled \
	-Ddefault-fonts-dirs="$fonts" -Dadditional-fonts-dirs=no -Dcache-dir=/var/cache/fontconfig
ninja -C build -j"$JOBS"
DESTDIR=/tmp/dest ninja -C build install
cp -a "/tmp/dest$LIVE/bin" "/tmp/dest$LIVE/etc" "/tmp/dest$LIVE/include" "/tmp/dest$LIVE/lib" \
	"/tmp/dest$LIVE/share" /out/
cp "$FILES/50-oath-families.conf" /out/etc/fonts/conf.d/
# CEF's own fontconfig reads this fonts.conf too, but resolves a relative
# include against its compiled-in /etc/fonts: name conf.d by its path.
sed -i "s|<include ignore_missing=\"yes\">conf.d</include>|<include ignore_missing=\"yes\">$LIVE/etc/fonts/conf.d</include>|" \
	/out/etc/fonts/fonts.conf
grep -q "$LIVE/etc/fonts/conf.d" /out/etc/fonts/fonts.conf
cd /out/bin
rm -f fc-cat fc-conflist fc-genconf fc-pattern fc-scan fc-validate
rm -rf /out/share/gettext /out/share/man
