#!/bin/sh
# Bootstrap the Oath toolchain from source. Run by `cargo make bootstrap`
# inside `nix-shell pkgs/bootstrap.nix` (host compilers, build-time only).
#
#   stage 0  host clang + lld (built by the host's gcc) that targets
#            x86_64-unknown-linux-musl, with a musl sysroot: Linux UAPI
#            headers, musl, compiler-rt builtins, libunwind, libc++abi,
#            libc++. Runs on the build host only; never ships.
#   stage 1  with stage 0: static musl LLVM (clang, lld, binutils
#            replacements) plus the same sysroot = pkg:llvm; static
#            busybox and GNU make. These three packs are what the stage-2
#            plans (pkgs/*/build.sh, run in the `oath build` sandbox) use.
#
# Inputs (env, set by cargo make): LLVM_SRC MUSL_SRC LINUX_SRC BUSYBOX_SRC
# MAKE_SRC (verified tarballs), WORK (scratch), OUT (stage-1 pack trees go
# to $OUT/{llvm,busybox,make}), JOBS. Each step leaves a stamp and is
# skipped when its stamp exists.
set -eu

: "${LLVM_SRC:?}" "${MUSL_SRC:?}" "${LINUX_SRC:?}" "${BUSYBOX_SRC:?}" "${MAKE_SRC:?}"
: "${WORK:?}" "${OUT:?}"
JOBS=${JOBS:-$(($(nproc) / 2))}
here=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
T=x86_64-unknown-linux-musl
S0=$WORK/stage0
SYS0=$S0/sysroot
mkdir -p "$WORK" "$OUT" "$S0" "$SYS0"

step() {
	if [ -f "$WORK/.done-$1" ]; then
		echo "== $1 (done)"
		return 1
	fi
	echo "== $1"
	return 0
}
done_() { touch "$WORK/.done-$1"; }

unpack() { # tarball dir
	if [ ! -d "$2" ]; then
		mkdir -p "$2.tmp"
		tar -xf "$1" -C "$2.tmp" --strip-components=1
		mv "$2.tmp" "$2"
	fi
}

unpack "$LLVM_SRC" "$WORK/llvm-project"
unpack "$MUSL_SRC" "$WORK/musl"
unpack "$LINUX_SRC" "$WORK/linux"
LLVM=$WORK/llvm-project

# LLVM_APPEND_VC_REV=OFF: the source tree sits inside the Oath checkout,
# and clang must not embed Oath's git revision (pkg:llvm stays reproducible).
common_llvm="-DCMAKE_BUILD_TYPE=Release
 -DLLVM_APPEND_VC_REV=OFF
 -DLLVM_TARGETS_TO_BUILD=X86
 -DLLVM_DEFAULT_TARGET_TRIPLE=$T
 -DCLANG_DEFAULT_RTLIB=compiler-rt
 -DCLANG_DEFAULT_UNWINDLIB=libunwind
 -DCLANG_DEFAULT_CXX_STDLIB=libc++
 -DCLANG_DEFAULT_LINKER=lld
 -DLLVM_INCLUDE_TESTS=OFF -DLLVM_INCLUDE_EXAMPLES=OFF
 -DLLVM_INCLUDE_BENCHMARKS=OFF -DLLVM_INCLUDE_DOCS=OFF
 -DCLANG_INCLUDE_TESTS=OFF -DCLANG_INCLUDE_DOCS=OFF
 -DLLVM_ENABLE_ZLIB=OFF -DLLVM_ENABLE_ZSTD=OFF -DLLVM_ENABLE_LIBXML2=OFF
 -DLLVM_ENABLE_TERMINFO=OFF -DLLVM_ENABLE_LIBEDIT=OFF -DLLVM_ENABLE_LIBPFM=OFF
 -DLLVM_ENABLE_BINDINGS=OFF -DCLANG_ENABLE_ARCMT=OFF
 -DCLANG_ENABLE_STATIC_ANALYZER=OFF -DLLVM_ENABLE_OCAMLDOC=OFF"
tools_targets="install-clang install-clang-resource-headers install-lld
 install-llvm-ar install-llvm-ranlib install-llvm-nm install-llvm-objcopy
 install-llvm-objdump install-llvm-readobj install-llvm-strip install-llvm-size
 install-llvm-strings install-llvm-symbolizer install-llvm-addr2line
 install-llvm-config"

# ---- stage 0 ----------------------------------------------------------

if step stage0-llvm; then
	# shellcheck disable=SC2086
	cmake -G Ninja -S "$LLVM/llvm" -B "$WORK/b0-llvm" $common_llvm \
		-DLLVM_ENABLE_PROJECTS="clang;lld" \
		-DDEFAULT_SYSROOT="$SYS0" \
		-DCMAKE_INSTALL_PREFIX="$S0"
	# shellcheck disable=SC2086
	ninja -C "$WORK/b0-llvm" -j"$JOBS" $tools_targets
	done_ stage0-llvm
fi
CC0="$S0/bin/clang"
CXX0="$S0/bin/clang++"
RES0=$("$CC0" -print-resource-dir)

if step stage0-linux-headers; then
	make -C "$WORK/linux" ARCH=x86_64 HOSTCC=gcc INSTALL_HDR_PATH="$SYS0" headers_install
	done_ stage0-linux-headers
fi

if step stage0-musl-headers; then
	(cd "$WORK/musl" && ./configure --prefix=/ --target=x86_64-linux-musl \
		--disable-shared CC="$CC0" AR="$S0/bin/llvm-ar" RANLIB="$S0/bin/llvm-ranlib" &&
		make install-headers DESTDIR="$SYS0")
	done_ stage0-musl-headers
fi

runtimes() { # build-dir install-prefix extra-cmake-args...
	b=$1
	prefix=$2
	shift 2
	cmake -G Ninja -S "$LLVM/runtimes" -B "$b" \
		-DCMAKE_BUILD_TYPE=Release \
		-DCMAKE_C_COMPILER="$CC0" -DCMAKE_CXX_COMPILER="$CXX0" -DCMAKE_ASM_COMPILER="$CC0" \
		-DCMAKE_AR="$S0/bin/llvm-ar" -DCMAKE_RANLIB="$S0/bin/llvm-ranlib" \
		-DCMAKE_NM="$S0/bin/llvm-nm" \
		-DCMAKE_C_COMPILER_TARGET=$T -DCMAKE_CXX_COMPILER_TARGET=$T \
		-DCMAKE_ASM_COMPILER_TARGET=$T \
		-DCMAKE_SYSROOT="$SYS0" \
		-DCMAKE_TRY_COMPILE_TARGET_TYPE=STATIC_LIBRARY \
		-DLLVM_DEFAULT_TARGET_TRIPLE=$T \
		-DLLVM_ENABLE_PER_TARGET_RUNTIME_DIR=ON \
		-DCMAKE_INSTALL_PREFIX="$prefix" "$@"
	ninja -C "$b" -j"$JOBS" install
}

if step stage0-builtins; then
	runtimes "$WORK/b0-builtins" "$RES0" \
		-DLLVM_ENABLE_RUNTIMES=compiler-rt \
		-DCOMPILER_RT_DEFAULT_TARGET_ONLY=ON \
		-DCOMPILER_RT_BUILD_BUILTINS=ON -DCOMPILER_RT_BUILD_CRT=ON \
		-DCOMPILER_RT_BUILD_SANITIZERS=OFF -DCOMPILER_RT_BUILD_XRAY=OFF \
		-DCOMPILER_RT_BUILD_LIBFUZZER=OFF -DCOMPILER_RT_BUILD_PROFILE=OFF \
		-DCOMPILER_RT_BUILD_MEMPROF=OFF -DCOMPILER_RT_BUILD_ORC=OFF \
		-DCOMPILER_RT_BUILD_CTX_PROFILE=OFF -DCOMPILER_RT_BUILD_GWP_ASAN=OFF \
		-DCOMPILER_RT_USE_LIBCXX=OFF \
		-DCMAKE_C_COMPILER_WORKS=ON -DCMAKE_CXX_COMPILER_WORKS=ON
	done_ stage0-builtins
fi

if step stage0-musl; then
	(cd "$WORK/musl" && make clean >/dev/null 2>&1 || true)
	(cd "$WORK/musl" && ./configure --prefix=/ --target=x86_64-linux-musl \
		--disable-shared CC="$CC0" AR="$S0/bin/llvm-ar" RANLIB="$S0/bin/llvm-ranlib" \
		LIBCC="$("$CC0" -print-libgcc-file-name)" &&
		make -j"$JOBS" && make install DESTDIR="$SYS0")
	done_ stage0-musl
fi

# libunwind, libc++abi and libc++ go straight into the sysroot's lib/,
# where clang looks for them (not a per-target subdirectory).
cxx_runtimes="-DLLVM_ENABLE_RUNTIMES=libunwind;libcxxabi;libcxx
 -DLLVM_ENABLE_PER_TARGET_RUNTIME_DIR=OFF
 -DLIBUNWIND_ENABLE_SHARED=OFF -DLIBUNWIND_USE_COMPILER_RT=ON
 -DLIBCXXABI_ENABLE_SHARED=OFF -DLIBCXXABI_USE_COMPILER_RT=ON
 -DLIBCXXABI_HAS_CXA_THREAD_ATEXIT_IMPL=OFF
 -DLIBCXXABI_USE_LLVM_UNWINDER=ON -DLIBCXXABI_ENABLE_STATIC_UNWINDER=ON
 -DLIBCXXABI_STATICALLY_LINK_UNWINDER_IN_STATIC_LIBRARY=ON
 -DLIBCXX_ENABLE_SHARED=OFF -DLIBCXX_USE_COMPILER_RT=ON
 -DLIBCXX_HAS_MUSL_LIBC=ON -DLIBCXX_ENABLE_STATIC_ABI_LIBRARY=ON
 -DLIBCXX_INCLUDE_BENCHMARKS=OFF -DLIBCXX_INCLUDE_TESTS=OFF
 -DLIBCXXABI_INCLUDE_TESTS=OFF -DLIBUNWIND_INCLUDE_TESTS=OFF
 -DLLVM_INCLUDE_TESTS=OFF"

if step stage0-cxx; then
	# shellcheck disable=SC2086
	runtimes "$WORK/b0-cxx" "$SYS0" $cxx_runtimes
	# clang finds libc++ headers next to itself (bin/../include/c++/v1).
	mkdir -p "$S0/include"
	rm -rf "$S0/include/c++"
	mv "$SYS0/include/c++" "$S0/include/c++"
	done_ stage0-cxx
fi

# ---- stage 1: pkg:llvm --------------------------------------------------

P=$OUT/llvm
if step stage1-llvm; then
	# shellcheck disable=SC2086
	cmake -G Ninja -S "$LLVM/llvm" -B "$WORK/b1-llvm" $common_llvm \
		-DLLVM_ENABLE_PROJECTS="clang;lld" \
		-DCMAKE_C_COMPILER="$CC0" -DCMAKE_CXX_COMPILER="$CXX0" -DCMAKE_ASM_COMPILER="$CC0" \
		-DCMAKE_AR="$S0/bin/llvm-ar" -DCMAKE_RANLIB="$S0/bin/llvm-ranlib" \
		-DCMAKE_NM="$S0/bin/llvm-nm" \
		-DLLVM_HOST_TRIPLE=$T \
		-DLLVM_BUILD_STATIC=ON -DLLVM_ENABLE_PIC=OFF \
		-DLLVM_ENABLE_LIBCXX=ON \
		-DCMAKE_EXE_LINKER_FLAGS="-static" \
		-DLLVM_ENABLE_THREADS=ON \
		-DCMAKE_INSTALL_PREFIX=/
	rm -rf "$P"
	# shellcheck disable=SC2086
	DESTDIR="$P" ninja -C "$WORK/b1-llvm" -j"$JOBS" $tools_targets
	done_ stage1-llvm
fi

if step stage1-llvm-sysroot; then
	# GNUInstallDirs puts a "/" prefix's files under usr/: flatten it.
	if [ -d "$P/usr" ]; then
		cp -a "$P/usr/." "$P/"
		rm -rf "$P/usr"
	fi
	RES1=$P/lib/clang/$(basename "$RES0")
	# Same runtimes as stage 0, rebuilt into the pack (identical code;
	# built by stage 0 for the target).
	runtimes "$WORK/b1-builtins" "$RES1" \
		-DLLVM_ENABLE_RUNTIMES=compiler-rt \
		-DCOMPILER_RT_DEFAULT_TARGET_ONLY=ON \
		-DCOMPILER_RT_BUILD_BUILTINS=ON -DCOMPILER_RT_BUILD_CRT=ON \
		-DCOMPILER_RT_BUILD_SANITIZERS=OFF -DCOMPILER_RT_BUILD_XRAY=OFF \
		-DCOMPILER_RT_BUILD_LIBFUZZER=OFF -DCOMPILER_RT_BUILD_PROFILE=OFF \
		-DCOMPILER_RT_BUILD_MEMPROF=OFF -DCOMPILER_RT_BUILD_ORC=OFF \
		-DCOMPILER_RT_BUILD_CTX_PROFILE=OFF -DCOMPILER_RT_BUILD_GWP_ASAN=OFF \
		-DCOMPILER_RT_USE_LIBCXX=OFF \
		-DCMAKE_C_COMPILER_WORKS=ON -DCMAKE_CXX_COMPILER_WORKS=ON
	rm -rf "$P/sysroot"
	mkdir -p "$P/sysroot"
	cp -a "$SYS0/include" "$SYS0/lib" "$P/sysroot/"
	# The pack's clang finds its sysroot next to itself and links
	# statically by default (there is no shared libc on Oath's base).
	for drv in clang clang++; do
		cat >"$P/bin/$drv.cfg" <<-'EOF'
			--sysroot=<CFGDIR>/../sysroot
			-static
			-Wno-unused-command-line-argument
		EOF
	done
	# Names build scripts expect.
	for pair in cc:clang c++:clang++ gcc:clang g++:clang++ cpp:clang-cpp \
		ar:llvm-ar ranlib:llvm-ranlib nm:llvm-nm objcopy:llvm-objcopy \
		objdump:llvm-objdump readelf:llvm-readelf strip:llvm-strip size:llvm-size \
		strings:llvm-strings addr2line:llvm-addr2line ld:ld.lld; do
		ln -sf "${pair#*:}" "$P/bin/${pair%%:*}"
	done
	[ -e "$P/bin/clang-cpp" ] || ln -sf clang "$P/bin/clang-cpp"
	[ -e "$P/bin/llvm-readelf" ] || ln -sf llvm-readobj "$P/bin/llvm-readelf"
	for drv in cc c++ gcc g++; do
		case $drv in c++ | g++) src=clang++ ;; *) src=clang ;; esac
		ln -sf "$src.cfg" "$P/bin/$drv.cfg"
	done
	rm -rf "$P/share" "$P/include" "$P/lib/cmake" "$P/lib"/*.a
	mkdir -p "$P/include"
	cp -a "$S0/include/c++" "$P/include/c++"
	done_ stage1-llvm-sysroot
fi

# ---- stage 1: busybox and make, built by stage 0 ---------------------------

S1CC="$CC0 -static"
if step stage1-busybox; then
	rm -rf "$WORK/busybox"
	unpack "$BUSYBOX_SRC" "$WORK/busybox"
	cd "$WORK/busybox"
	cp "$here/busybox/config" .config
	make HOSTCC=gcc CC="$S1CC" AR="$S0/bin/llvm-ar" STRIP="$S0/bin/llvm-strip" oldconfig </dev/null
	make HOSTCC=gcc CC="$S1CC" AR="$S0/bin/llvm-ar" STRIP="$S0/bin/llvm-strip" -j"$JOBS" busybox
	rm -rf "$OUT/busybox"
	mkdir -p "$OUT/busybox/bin"
	cp busybox "$OUT/busybox/bin/busybox"
	for a in $(./busybox --list); do
		[ "$a" = busybox ] || ln -sf busybox "$OUT/busybox/bin/$a"
	done
	cd - >/dev/null
	done_ stage1-busybox
fi

if step stage1-make; then
	rm -rf "$WORK/make"
	unpack "$MAKE_SRC" "$WORK/make"
	cd "$WORK/make"
	./configure --prefix=/ --disable-nls --without-guile CC="$S1CC" \
		AR="$S0/bin/llvm-ar" RANLIB="$S0/bin/llvm-ranlib"
	sh ./build.sh
	rm -rf "$OUT/make"
	mkdir -p "$OUT/make/bin"
	cp make "$OUT/make/bin/make"
	ln -sf make "$OUT/make/bin/gmake"
	cd - >/dev/null
	done_ stage1-make
fi

echo "stage-1 packs: $OUT/llvm $OUT/busybox $OUT/make"
