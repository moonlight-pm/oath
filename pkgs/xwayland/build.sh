# Xwayland with glamor on GBM (DRI3) and GLX, SHA-1 from nettle and the
# built-in fonts. Keymaps are compiled by xkbcomp from xkeyboard-config,
# both in their slots. No XDMCP, secure RPC (libtirpc), libdecor or
# libei. GLX links libGL, pkg:libglvnd's (R34), which sends each client
# to the vendor this server names: Mesa's, or NVIDIA's on its GBM.
tar -xf "$SRC/$SOURCES"
cd "xwayland-$VERSION"
meson setup build --prefix="$LIVE" --libdir=lib --buildtype=release \
	-Dglamor=true -Ddri3=true -Ddrm=true -Dglx=true -Dsha1=libnettle -Dxvfb=false \
	-Dxwayland_ei=false -Dlibdecor=false -Dxdmcp=false -Dxdm-auth-1=false -Dsecure-rpc=false \
	-Dxcsecurity=false -Dxf86bigfont=false -Dxselinux=false -Dlisten_tcp=false \
	-Dsystemd_notify=false -Dlibunwind=false -Ddtrace=false \
	-Ddocs=false -Ddevel-docs=false -Ddocs-pdf=false \
	-Dxkb_dir=/oath/store/pkg/xkeyboard-config/live/share/X11/xkb \
	-Dxkb_bin_dir=/oath/store/pkg/xkbcomp/live/bin -Ddefault_font_path=built-ins
grep -q '#define XWL_HAS_GLAMOR 1' build/include/xwayland-config.h || { echo "no glamor (gbm?)"; exit 1; }
ninja -C build -j"$JOBS"
DESTDIR=/tmp/dest ninja -C build install
cp -a "/tmp/dest$LIVE/bin" "/tmp/dest$LIVE/lib" /out/
