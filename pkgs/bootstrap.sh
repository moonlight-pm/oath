#!/bin/sh
# Bootstrap the Oath toolchain from source (R17). Run by `cargo make
# bootstrap` inside `nix-shell pkgs/bootstrap.nix` (host compilers,
# build-time only).
#
#   stage 0  with the host's gcc: a cross toolchain for $T0 (binutils,
#            GCC C/C++) and its sysroot (Linux UAPI headers, glibc,
#            libgcc, libstdc++). Runs on the build host only; never ships.
#   stage 1  with stage 0: pkg:gcc (native GCC and binutils for $T),
#            pkg:glibc (the sysroot's glibc and headers) and the GNU tools
#            (bash, coreutils, sed, grep, gawk, findutils, diffutils,
#            patch, tar, gzip, xz, bzip2, make). These are the base every
#            stage-2 plan (pkgs/*/build.sh, run in the `oath build`
#            sandbox) builds with.
#
# Every program links against glibc's loader at /lib/ld-linux-x86-64.so.2
# (no /lib64), and finds libraries in /lib, the sandbox's and a
# machine's link farm. pkg:gcc reads glibc's headers from
# /oath/store/pkg/glibc/live/include.
#
# Inputs (env, set by cargo make): GCC_SRC BINUTILS_SRC GMP_SRC MPFR_SRC
# MPC_SRC GLIBC_SRC LINUX_SRC and SRC_<tool> for each of TOOLS (verified
# tarballs), WORK (scratch), OUT (stage-1 pack trees go to $OUT/<name>),
# JOBS. Each step leaves a stamp and is skipped when its stamp exists.
set -eu

: "${GCC_SRC:?}" "${BINUTILS_SRC:?}" "${GMP_SRC:?}" "${MPFR_SRC:?}" "${MPC_SRC:?}"
: "${GLIBC_SRC:?}" "${LINUX_SRC:?}" "${TOOLS:?}"
: "${WORK:?}" "${OUT:?}"
JOBS=${JOBS:-$(($(nproc) / 2))}
here=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
# Stage 0's own triple keeps it a cross toolchain: nothing of the host's
# can leak into what it builds. Everything that ships is $T.
T0=x86_64-oath-linux-gnu
T=x86_64-pc-linux-gnu
S0=$WORK/stage0
SYS0=$S0/sysroot
GLIBC_HEADERS=/oath/store/pkg/glibc/live/include
LOADER=/lib/ld-linux-x86-64.so.2
BUILD=$(gcc -dumpmachine)
mkdir -p "$WORK" "$OUT" "$S0" "$SYS0"
export PATH="$S0/bin:$PATH"

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

unpack "$GCC_SRC" "$WORK/gcc"
unpack "$BINUTILS_SRC" "$WORK/binutils"
unpack "$GMP_SRC" "$WORK/gmp"
unpack "$MPFR_SRC" "$WORK/mpfr"
unpack "$MPC_SRC" "$WORK/mpc"
unpack "$GLIBC_SRC" "$WORK/glibc"
unpack "$LINUX_SRC" "$WORK/linux"

if step gcc-tree; then
	# GMP, MPFR and MPC build inside GCC's tree, static, into cc1.
	for d in gmp mpfr mpc; do ln -sfn "$WORK/$d" "$WORK/gcc/$d"; done
	# No /lib64: the loader is /lib/ld-linux-x86-64.so.2, and libraries
	# install to lib/, not lib64/.
	sed -i 's|/lib64/ld-linux-x86-64.so.2|/lib/ld-linux-x86-64.so.2|' \
		"$WORK/gcc/gcc/config/i386/linux64.h"
	sed -i '/m64=/s/lib64/lib/' "$WORK/gcc/gcc/config/i386/t-linux64"
	done_ gcc-tree
fi

# GMP 6.3.0 predates C23, GCC 16's default: its configure tests need C17.
host_cflags="-O2 -std=gnu17"

# ---- stage 0: the cross toolchain ---------------------------------------

if step stage0-binutils; then
	# Stage 0 starts here: from an empty prefix and sysroot, so nothing
	# left from an earlier toolchain (or an earlier libc) can mix in.
	rm -rf "$S0" && mkdir -p "$SYS0"
	rm -rf "$WORK/b0-binutils" && mkdir -p "$WORK/b0-binutils"
	(cd "$WORK/b0-binutils" && "$WORK/binutils/configure" \
		--target=$T0 --prefix="$S0" --with-sysroot="$SYS0" --with-lib-path==/lib \
		--disable-nls --disable-werror --disable-gprofng --disable-gdb \
		--disable-gdbserver --disable-sim --disable-readline --disable-libdecnumber \
		--enable-default-hash-style=gnu MAKEINFO=true &&
		make -j"$JOBS" MAKEINFO=true && make install MAKEINFO=true)
	done_ stage0-binutils
fi

common_gcc="--disable-nls --disable-multilib --disable-bootstrap
 --enable-default-pie --enable-default-ssp --enable-languages=c,c++
 --disable-libsanitizer --disable-libssp --disable-libvtv --disable-libitm
 --disable-libquadmath --without-isl --without-zstd"

if step stage0-gcc1; then
	# C only and no C library yet: enough to build glibc.
	rm -rf "$WORK/b0-gcc1" && mkdir -p "$WORK/b0-gcc1"
	# shellcheck disable=SC2086
	(cd "$WORK/b0-gcc1" && CFLAGS="$host_cflags" "$WORK/gcc/configure" \
		--target=$T0 --prefix="$S0" --with-sysroot="$SYS0" $common_gcc \
		--with-native-system-header-dir=/include \
		--with-glibc-version=2.44 --with-newlib --without-headers \
		--disable-shared --disable-threads --disable-libatomic --disable-libgomp \
		--disable-libstdcxx --enable-languages=c MAKEINFO=true &&
		make -j"$JOBS" all-gcc all-target-libgcc MAKEINFO=true &&
		make install-gcc install-target-libgcc MAKEINFO=true)
	done_ stage0-gcc1
fi

if step stage0-linux-headers; then
	make -C "$WORK/linux" ARCH=x86_64 HOSTCC=gcc INSTALL_HDR_PATH="$SYS0" headers_install
	done_ stage0-linux-headers
fi

# glibc lives at the root: headers in /include, libraries and the loader
# in /lib. --prefix= is empty (Oath has no /usr).
glibc_configure() { # build-dir cc
	rm -rf "$1" && mkdir -p "$1"
	cat >"$1/configparms" <<-EOF
		slibdir=/lib
		rtlddir=/lib
	EOF
	(cd "$1" && "$WORK/glibc/configure" --host=$T --build="$BUILD" --prefix= \
		--libdir=/lib --libexecdir=/lib/libexec --with-headers="$SYS0/include" \
		--enable-kernel=5.10 --disable-werror --disable-nscd --disable-timezone-tools \
		--enable-stack-protector=strong --enable-bind-now \
		CC="$2" CXX=false AR=$T0-ar RANLIB=$T0-ranlib AS=$T0-as LD=$T0-ld \
		NM=$T0-nm OBJCOPY=$T0-objcopy OBJDUMP=$T0-objdump READELF=$T0-readelf \
		libc_cv_slibdir=/lib libc_cv_rtlddir=/lib)
}

if step stage0-glibc; then
	glibc_configure "$WORK/b0-glibc" "$T0-gcc"
	make -C "$WORK/b0-glibc" -j"$JOBS"
	make -C "$WORK/b0-glibc" install DESTDIR="$SYS0"
	done_ stage0-glibc
fi

if step stage0-gcc2; then
	# The whole cross compiler: shared libgcc, libstdc++, threads.
	rm -rf "$WORK/b0-gcc2" && mkdir -p "$WORK/b0-gcc2"
	# shellcheck disable=SC2086
	(cd "$WORK/b0-gcc2" && CFLAGS="$host_cflags" "$WORK/gcc/configure" \
		--target=$T0 --prefix="$S0" --with-sysroot="$SYS0" $common_gcc \
		--with-native-system-header-dir=/include \
		--enable-shared --enable-threads=posix --enable-__cxa_atexit \
		--disable-libgomp MAKEINFO=true &&
		make -j"$JOBS" MAKEINFO=true && make install MAKEINFO=true)
	done_ stage0-gcc2
fi

# ---- stage 1: pkg:glibc ---------------------------------------------------

G=$OUT/glibc
if step stage1-glibc; then
	# Stage 0's glibc, as built: stage 2's plan rebuilds it with pkg:gcc.
	rm -rf "$G" && mkdir -p "$G"
	cp -a "$SYS0/include" "$SYS0/lib" "$G/"
	for d in etc share var sbin bin; do
		[ -d "$SYS0/$d" ] && cp -a "$SYS0/$d" "$G/"
	done
	done_ stage1-glibc
fi

# ---- stage 1: pkg:gcc, a native compiler for $T built by stage 0 ----------

P=$OUT/gcc
cross="CC=$T0-gcc CXX=$T0-g++ AR=$T0-ar RANLIB=$T0-ranlib NM=$T0-nm
 STRIP=$T0-strip OBJCOPY=$T0-objcopy OBJDUMP=$T0-objdump READELF=$T0-readelf
 AS=$T0-as LD=$T0-ld"

if step stage1-binutils; then
	rm -rf "$WORK/b1-binutils" && mkdir -p "$WORK/b1-binutils"
	# shellcheck disable=SC2086
	(cd "$WORK/b1-binutils" && env $cross "$WORK/binutils/configure" \
		--build="$BUILD" --host=$T --target=$T --prefix= --with-lib-path=/lib \
		--disable-nls --disable-werror --disable-gprofng --disable-gdb \
		--disable-gdbserver --disable-sim --disable-readline --disable-libdecnumber \
		--enable-default-hash-style=gnu --enable-plugins --enable-ld=default \
		MAKEINFO=true &&
		make -j"$JOBS" MAKEINFO=true)
	rm -rf "$P"
	make -C "$WORK/b1-binutils" install DESTDIR="$P" MAKEINFO=true
	done_ stage1-binutils
fi

if step stage1-gcc; then
	# While it builds, the new compiler finds glibc's headers where it
	# will on Oath, under the build sysroot.
	mkdir -p "$SYS0${GLIBC_HEADERS%/include}"
	ln -sfn "$SYS0/include" "$SYS0$GLIBC_HEADERS"
	rm -rf "$WORK/b1-gcc" && mkdir -p "$WORK/b1-gcc"
	# shellcheck disable=SC2086
	# The target libraries (libgcc, libstdc++) build as GCC 16 expects,
	# not with the host's -std=gnu17.
	(cd "$WORK/b1-gcc" && env $cross CFLAGS="$host_cflags" CXXFLAGS=-O2 \
		CFLAGS_FOR_TARGET=-O2 CXXFLAGS_FOR_TARGET=-O2 \
		CC_FOR_TARGET=$T0-gcc CXX_FOR_TARGET=$T0-g++ GCC_FOR_TARGET=$T0-gcc \
		AR_FOR_TARGET=$T0-ar AS_FOR_TARGET=$T0-as LD_FOR_TARGET=$T0-ld \
		NM_FOR_TARGET=$T0-nm RANLIB_FOR_TARGET=$T0-ranlib \
		STRIP_FOR_TARGET=$T0-strip OBJDUMP_FOR_TARGET=$T0-objdump \
		READELF_FOR_TARGET=$T0-readelf \
		"$WORK/gcc/configure" --build="$BUILD" --host=$T --target=$T --prefix= \
		$common_gcc --with-native-system-header-dir=$GLIBC_HEADERS \
		--with-build-sysroot="$SYS0" --enable-shared --enable-threads=posix \
		--enable-__cxa_atexit --enable-lto \
		--with-gcc-major-version-only MAKEINFO=true &&
		make -j"$JOBS" MAKEINFO=true)
	make -C "$WORK/b1-gcc" install DESTDIR="$P" MAKEINFO=true
	done_ stage1-gcc
fi

if step stage1-gcc-pack; then
	# Names build scripts expect, and binutils' plugin directory for LTO.
	for pair in cc:gcc c++:g++; do
		ln -sf "${pair#*:}" "$P/bin/${pair%%:*}"
	done
	mkdir -p "$P/lib/bfd-plugins"
	ln -sf "../../libexec/gcc/$T/16/liblto_plugin.so" "$P/lib/bfd-plugins/liblto_plugin.so"
	# binutils' tooldir copies and docs are not needed.
	rm -rf "$P/$T" "$P/share" "$P/include/"*.h
	find "$P/lib" -name '*.la' -delete
	done_ stage1-gcc-pack
fi

# ---- stage 1: the GNU tools, built by stage 0 -------------------------------

# One GNU tool for each job (R18), installed under the slot path plans
# use them at, so their compiled-in paths (gawk's AWKPATH, bash's
# defaults) hold in the sandbox. Each is cross-built: none can run here.
tool() { # name [configure args...]
	n=$1
	shift
	step "stage1-$n" || return 0
	eval "src=\$SRC_$n"
	rm -rf "$WORK/t-$n" "$WORK/d-$n"
	unpack "$src" "$WORK/t-$n"
	live=/oath/store/pkg/$n/live
	cd "$WORK/t-$n"
	case $n in
	gawk) sed -i 's/extras//' Makefile.in ;;
	esac
	./configure --build="$BUILD" --host=$T --prefix=$live --disable-nls "$@" \
		CC=$T0-gcc AR=$T0-ar RANLIB=$T0-ranlib
	make -j"$JOBS"
	make install DESTDIR="$WORK/d-$n"
	cd - >/dev/null
	rm -rf "$OUT/$n"
	mv "$WORK/d-$n$live" "$OUT/$n"
	rm -rf "$OUT/$n/share/info" "$OUT/$n/share/man" "$OUT/$n/share/doc" "$OUT/$n/share/locale"
	done_ "stage1-$n"
}

tool bash --without-bash-malloc bash_cv_strtold_broken=no
[ -e "$OUT/bash/bin/sh" ] || ln -s bash "$OUT/bash/bin/sh"
tool coreutils --enable-install-program=hostname --enable-no-install-program=kill,uptime \
	gl_cv_macro_MB_CUR_MAX_good=y
tool sed
tool grep
tool gawk
tool findutils --localstatedir=/var/lib/locate
tool diffutils gl_cv_func_strcasecmp_works=y
tool patch
tool tar
tool gzip
tool xz --disable-static --disable-doc
tool make --without-guile

if step stage1-bzip2; then
	# bzip2 has no configure; its `all` runs the new binary, which cannot
	# run here.
	rm -rf "$WORK/t-bzip2"
	unpack "$SRC_bzip2" "$WORK/t-bzip2"
	cd "$WORK/t-bzip2"
	b="CC=$T0-gcc AR=$T0-ar RANLIB=$T0-ranlib"
	# shellcheck disable=SC2086
	make -f Makefile-libbz2_so $b
	make clean
	# shellcheck disable=SC2086
	make $b libbz2.a bzip2 bzip2recover
	rm -rf "$OUT/bzip2"
	mkdir -p "$OUT/bzip2/bin" "$OUT/bzip2/lib" "$OUT/bzip2/include"
	cp bzip2-shared "$OUT/bzip2/bin/bzip2"
	cp bzip2recover "$OUT/bzip2/bin/"
	ln -s bzip2 "$OUT/bzip2/bin/bunzip2"
	ln -s bzip2 "$OUT/bzip2/bin/bzcat"
	cp -a libbz2.so.1.0.8 "$OUT/bzip2/lib/"
	ln -s libbz2.so.1.0.8 "$OUT/bzip2/lib/libbz2.so.1.0"
	ln -s libbz2.so.1.0.8 "$OUT/bzip2/lib/libbz2.so"
	cp libbz2.a "$OUT/bzip2/lib/"
	cp bzlib.h "$OUT/bzip2/include/"
	cd - >/dev/null
	done_ stage1-bzip2
fi

echo "stage-1 packs in $OUT: gcc glibc $TOOLS"
