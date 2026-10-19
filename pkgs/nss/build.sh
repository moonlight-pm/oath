# NSS's libraries (libnss3 opens softokn, freebl and nssckbi beside
# itself, with their .chk files), headers and nss.pc. NSS has no install
# target; its object directory is named after the host, so it is found
# by pattern. Serial: coreconf's recursive make. coreconf ignores
# CFLAGS; XCFLAGS carries the include paths (zlib).
tar -xf "$SRC/$SOURCES"
cd "nss-$VERSION/nss"
nspr=/oath/store/pkg/nspr/live
make all BUILD_OPT=1 USE_64=1 NSPR_INCLUDE_DIR="$nspr/include/nspr" NSPR_LIB_DIR="$nspr/lib" \
	USE_SYSTEM_ZLIB=1 ZLIB_LIBS=-lz NSS_DISABLE_GTESTS=1 NSS_ENABLE_WERROR=0 CC=cc CCC=c++ \
	XCFLAGS="$CPPFLAGS"
obj=$(echo ../dist/Linux*_x86_64_cc_glibc_PTH_64_OPT.OBJ)
mkdir -p /out/lib/pkgconfig /out/include/nss
cp "$obj"/lib/*.so "$obj"/lib/*.chk /out/lib/
cp -RL ../dist/public/nss/. /out/include/nss/
nspr_version=$(sed -n 's/^Version: //p' "$nspr/lib/pkgconfig/nspr.pc")
sed -e "s|%prefix%|$LIVE|" -e "s|%exec_prefix%|$LIVE|" -e "s|%libdir%|$LIVE/lib|" \
	-e "s|%includedir%|$LIVE/include/nss|" -e "s|%NSS_VERSION%|$VERSION|" \
	-e "s|%NSPR_VERSION%|$nspr_version|" pkg/pkg-config/nss.pc.in >/out/lib/pkgconfig/nss.pc
