# PulseAudio's client: libpulse (and its libpulsecommon), libpulse-simple,
# libpulse-mainloop-glib, pactl and pacat. No daemon, D-Bus, X11, ALSA or
# OSS wrapper; client.conf does not autospawn a server.
tar -xf "$SRC/$SOURCES"
cd "pulseaudio-$VERSION"
find . -name '*.py' -exec chmod -x {} +
meson setup build --prefix="$LIVE" --libdir=lib --buildtype=release --wrap-mode=nodownload \
	-Ddaemon=false -Dclient=true -Ddoxygen=false -Dman=false -Dtests=false -Dglib=enabled \
	-Ddbus=disabled -Dx11=disabled -Dsystemd=disabled -Delogind=disabled -Dconsolekit=disabled \
	-Dbluez5=disabled -Dgtk=disabled -Dorc=disabled -Davahi=disabled -Dtcpwrap=disabled \
	-Dgsettings=disabled -Dudev=disabled -Dalsa=disabled -Djack=disabled -Dlirc=disabled \
	-Dopenssl=disabled -Dfftw=disabled -Dsoxr=disabled -Dspeex=disabled -Dwebrtc-aec=disabled \
	-Dgstreamer=disabled -Dbluez5-gstreamer=disabled -Dasyncns=disabled -Doss-output=disabled \
	-Dvalgrind=disabled -Dbashcompletiondir=no -Dzshcompletiondir=no
ninja -C build -j"$JOBS"
DESTDIR=/tmp/dest ninja -C build install
cp -a "/tmp/dest$LIVE/." /out/
rm -rf /out/lib/cmake /out/share/locale /out/share/vala
# libpulsecommon sits in lib/pulseaudio behind a RUNPATH; on a machine
# libraries are found in /lib, so it goes beside the others.
mv /out/lib/pulseaudio/libpulsecommon-$VERSION.so /out/lib/
rmdir /out/lib/pulseaudio
sed -i 's/^; autospawn = yes$/autospawn = no/' /out/etc/pulse/client.conf
grep -q '^autospawn = no$' /out/etc/pulse/client.conf
