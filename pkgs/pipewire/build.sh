# PipeWire with ALSA (devices through libudev-zero), pipewire-pulse (the
# pulse protocol, no libpulse), the ALSA plugin, the tools and Bluetooth
# audio through BlueZ (A2DP with SBC, HSP and HFP of its own); no session
# manager (WirePlumber is its own pack), JACK, video, systemd or docs. Options that turn themselves on when a library is present
# (X11, GSettings, Flatpak) are turned off.
tar -xf "$SRC/$SOURCES"
cd "pipewire-$VERSION"
find . -name '*.py' -exec chmod -x {} +
meson setup build --prefix="$LIVE" --libdir=lib --buildtype=release --wrap-mode=nodownload \
	-Dsession-managers=[] -Ddocs=disabled -Dman=disabled -Dexamples=disabled -Dtests=disabled \
	-Dinstalled_tests=disabled -Dgstreamer=disabled -Dgstreamer-device-provider=disabled \
	-Dlibsystemd=disabled -Dlogind=disabled -Dsystemd-system-service=disabled \
	-Dsystemd-user-service=disabled -Dselinux=disabled -Dpipewire-alsa=enabled -Dalsa=enabled \
	-Dudev=enabled -Dudevrulesdir="$LIVE/lib/udev/rules.d" -Dpipewire-jack=disabled \
	-Djack=disabled -Dpipewire-v4l2=disabled -Dv4l2=disabled -Dlibcamera=disabled \
	-Dbluez5=enabled -Dbluez5-backend-native-mm=disabled -Dbluez5-backend-ofono=disabled \
	-Dbluez5-backend-hsphfpd=disabled -Dbluez5-codec-aptx=disabled -Dbluez5-codec-ldac=disabled \
	-Dbluez5-codec-ldac-dec=disabled -Dbluez5-codec-aac=disabled -Dbluez5-codec-lc3plus=disabled \
	-Dbluez5-codec-opus=disabled -Dbluez5-codec-lc3=disabled -Dbluez5-plc-spandsp=disabled \
	-Dffmpeg=disabled -Dpw-cat-ffmpeg=disabled -Dvulkan=disabled \
	-Dsdl2=disabled -Dvideoconvert=disabled -Dvideotestsrc=disabled -Dvolume=disabled \
	-Dtest=disabled -Devl=disabled -Dpw-cat=enabled -Dsndfile=enabled -Dlibpulse=disabled \
	-Droc=disabled -Davahi=disabled -Decho-cancel-webrtc=disabled -Dlibusb=disabled \
	-Draop=disabled -Dlv2=disabled -Dlibmysofa=disabled -Dx11=disabled -Dx11-xfixes=disabled \
	-Dlibcanberra=disabled -Dlegacy-rtkit=false -Davb=disabled -Dflatpak=disabled \
	-Dsnap=disabled -Dreadline=disabled -Dgsettings=disabled -Dgsettings-pulse-schema=disabled \
	-Dcompress-offload=disabled -Drlimits-install=false -Dopus=disabled -Dlibffado=disabled \
	-Debur128=disabled -Dfftw=disabled -Donnxruntime=disabled -Ddbus=enabled
ninja -C build -j"$JOBS"
DESTDIR=/tmp/dest ninja -C build install
cp -a "/tmp/dest$LIVE/." /out/
rm -rf /out/lib/udev /out/lib/cmake /out/share/locale
[ -f /out/lib/spa-0.2/bluez5/libspa-bluez5.so ]
