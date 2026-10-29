# Chromium with CEF's patches, built as CEF's own builds are (is_official_build,
# ThinLTO, CFI) with proprietary_codecs and ffmpeg_branding="Chrome": Chromium's
# FFmpeg then decodes H.264 and AAC, and its demuxers read MP4 (HEVC Chromium
# decodes only in hardware, through VA-API, which is off). The toolchain is
# Chromium's (R40), unpacked where `gclient` puts it, its loader set to /lib's:
# Clang and lld, the Rust toolchain, node, and the gn, esbuild and clang-format
# in the tarball. There is no sysroot (Chromium's is Debian's, R7): Chromium's
# unbundle toolchain hands clang glibc's headers after its own, GCC's crt files
# and libgcc, and /lib's loader for what the build runs; libraries come from
# pkg-config and /lib. libcef.so carries Chromium's libc++, as the vendor's does.
# patches/: CEF reads its version from git, which a tarball lacks, so git_util
# answers from CEF_TARBALL_COMMIT (the hash and commit number of CEF's 156.0.14);
# Oath's Python has no POSIX semaphores (its configure found no /dev/shm), so
# the mojom parser and Blink's bindings generator run in one process; fontconfig
# and ANGLE link libuuid and libXi, which they never call and Oath does not ship.
#
# The layout is the vendor's minimal distribution's: cef/Release (libcef.so,
# SwiftShader, the V8 snapshot) and cef/Resources, whose files are also linked
# into Release, where CEF looks beside libcef.so. lib/libcef.so is for the load
# check: sola-browser loads libcef.so by RUNPATH from Release, beside what CEF
# opens from there. No chrome-sandbox: Sola runs CEF without its setuid sandbox.
# No libvulkan.so.1: Chromium's loader searches /etc/xdg and /usr/share, which
# hold no manifests here, and a program with Release in its RUNPATH would load
# it instead of /lib's (pkg:vulkan-loader), which finds Mesa's; SwiftShader's
# manifest stays and works through that.
CEF_COMMIT=4fa4ab516dacf6c49fc7e6e70d2b7ebce29fa35f
CEF_NUMBER=3659
CHROMIUM=${VERSION##*chromium-}
CEF_PATCH=${VERSION%%+*}
CEF_PATCH=${CEF_PATCH##*.}
tar -xf "$SRC/chromium-$CHROMIUM.tar.xz" -C /tmp
mv "/tmp/chromium-$CHROMIUM" /tmp/src
cd /tmp/src
tar -xf "$SRC/cef-$CEF_COMMIT.tar.gz"
mv "cef-$CEF_COMMIT" cef
mkdir -p third_party/llvm-build/Release+Asserts third_party/rust-toolchain \
	third_party/node/linux third_party/test_fonts/test_fonts
tar -xf "$SRC"/clang-llvmorg-*.tar.xz -C third_party/llvm-build/Release+Asserts
tar -xf "$SRC"/rust-toolchain-*.tar.xz -C third_party/rust-toolchain
tar -xf "$SRC"/node-linux-x64-*.tar.gz -C third_party/node/linux
tar -xf "$SRC"/test_fonts-*.tar.gz -C third_party/test_fonts/test_fonts
for f in third_party/llvm-build/Release+Asserts/bin/* third_party/rust-toolchain/bin/* \
	third_party/rust-toolchain/libexec/* third_party/node/linux/node-linux-x64/bin/node \
	buildtools/linux64/gn buildtools/linux64-format/clang-format \
	third_party/devtools-frontend/src/third_party/esbuild/esbuild; do
	[ -f "$f" ] && [ ! -L "$f" ] || continue
	patchelf --print-interpreter "$f" >/dev/null 2>&1 || continue
	patchelf --set-interpreter /lib/ld-linux-x86-64.so.2 "$f"
done
for p in "$FILES"/patches/*.patch; do
	echo "patch ${p##*/}"
	patch -p1 -i "$p"
done
# protoc runs these plugins by their #! line, /usr/bin/env, and there is no /usr.
sed -i '1s|^#!/usr/bin/env python3$|#!/bin/python3|' tools/protoc_wrapper/protoc-gen-ts_proto.py \
	third_party/dom_distiller_js/protoc_plugins/json_values_converter.py
# gn's Python (for gn gen and V8's `gn desc`) and Blink's gperf are Oath's, not
# the CIPD binaries in the tarball.
sed -i 's|^script_executable = .*|script_executable = "/bin/python3"|' .gn
ln -sf /bin/gperf third_party/gperf/cipd/bin/gperf

# Chromium includes some headers without asking pkg-config (xcb, X11,
# xkbcommon, expat, ffi), from where a distribution keeps them, /usr/include:
# glibc's and every pack's include/ stand in for it, after Chromium's own and
# clang's. bindgen's libclang takes the same.
SYS="-idirafter /oath/store/pkg/glibc/live/include"
for i in $CPPFLAGS; do
	SYS="$SYS -idirafter ${i#-I}"
done
# Chromium's printing asks cups-config, which libcups' pack does not ship;
# libcups' pkg-config file has the same answers.
mkdir -p /tmp/bin
cat > /tmp/bin/cups-config <<'EOF'
#!/bin/sh
case "$1" in
--api-version) pkg-config --modversion cups | cut -d. -f1,2 ;;
*) pkg-config --cflags --libs cups ;;
esac
EOF
chmod +x /tmp/bin/cups-config
LLVM=/tmp/src/third_party/llvm-build/Release+Asserts/bin
export PATH="/tmp/bin:/tmp/src/buildtools/linux64:$PATH"
export CC="$LLVM/clang" CXX="$LLVM/clang++" AR="$LLVM/llvm-ar" NM="$LLVM/llvm-nm"
export CFLAGS="$SYS" CXXFLAGS="$SYS" CPPFLAGS= BINDGEN_EXTRA_CLANG_ARGS="$SYS"
export LDFLAGS="--gcc-install-dir=/oath/store/pkg/gcc/live/lib/gcc/x86_64-pc-linux-gnu/16 \
-Wl,--dynamic-linker=/lib/ld-linux-x86-64.so.2"
export BUILD_CC="$CC" BUILD_CXX="$CXX" BUILD_AR="$AR" BUILD_NM="$NM"
export BUILD_CFLAGS="$CFLAGS" BUILD_CXXFLAGS="$CXXFLAGS" BUILD_CPPFLAGS= BUILD_LDFLAGS="$LDFLAGS"
export CEF_TARBALL_COMMIT="$CEF_COMMIT $CEF_NUMBER $CEF_PATCH"
# Oath's libffi, not the static libffi_pic Chromium takes from Debian's sysroot.
# No WebRTC screen capture of an X11 session (libXtst and libXi, which Oath does
# not ship): the desk is Wayland. V8's instance types come from Torque, not
# from metagen, whose libclang reads flags from `gn desc`, without the
# toolchain's include dirs. Crashpad uploads over its own socket and BoringSSL,
# as on Android, not libcurl. No OpenXR: there is no XR runtime here for its
# loader to find.
export GN_OUT_CONFIGS=Release_GN_x64
export GN_DEFINES="is_official_build=true proprietary_codecs=true ffmpeg_branding=Chrome \
chrome_pgo_phase=0 symbol_level=0 blink_symbol_level=0 v8_symbol_level=0 \
treat_warnings_as_errors=false use_sysroot=false use_siso=false \
custom_toolchain=\"//build/toolchain/linux/unbundle:default\" \
host_toolchain=\"//build/toolchain/linux/unbundle:default\" \
use_vaapi=false use_gtk=false cef_use_gtk=false use_kerberos=false use_libpci=false \
use_system_libffi=true rtc_use_x11_extensions=false v8_use_metagen_instance_types=false \
crashpad_http_transport_impl=\"socket\" enable_openxr=false"

# What CEF's gclient hook does, without depot_tools: CEF's generated files,
# its patches to Chromium (by `patch`, as the tree is no checkout), args.gn.
(cd cef && python3 tools/version_manager.py -u --fast-check && python3 tools/patcher.py)
python3 - <<'EOF'
import os, sys
sys.path.insert(0, 'cef/tools')
from gn_args import GetAllPlatformConfigs, GetConfigFileContents
for d, c in GetAllPlatformConfigs({}).items():
    os.makedirs('out/' + d, exist_ok=True)
    open('out/%s/args.gn' % d, 'w').write(GetConfigFileContents(c) + '\n')
EOF
gn gen out/Release_GN_x64

# Build.
ninja -C out/Release_GN_x64 -j"$JOBS" cefsimple

o=out/Release_GN_x64
mkdir -p /out/cef/Release /out/cef/Resources/locales /out/lib
cp "$o/libcef.so" "$o/libvk_swiftshader.so" "$o/vk_swiftshader_icd.json" \
	"$o/v8_context_snapshot.bin" /out/cef/Release/
cp "$o/chrome_100_percent.pak" "$o/chrome_200_percent.pak" "$o/resources.pak" \
	"$o/icudtl.dat" /out/cef/Resources/
cp "$o"/locales/*.pak /out/cef/Resources/locales/
cp cef/LICENSE.txt /out/cef/
cp "$o/gen/components/resources/about_credits.html" /out/cef/CREDITS.html
for f in /out/cef/Resources/*; do
	ln -s "../Resources/${f##*/}" "/out/cef/Release/${f##*/}"
done
ln -s ../cef/Release/libcef.so /out/lib/libcef.so
