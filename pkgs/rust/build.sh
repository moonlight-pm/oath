# Output: bin/{rustc,rustdoc,cargo}, lib/ (librustc_driver, the loader,
# libgcc_s.so.1, rustlib/), etc/ld-musl-x86_64.path, share/doc/rust/.
host=x86_64-unknown-linux-musl
for c in rustc-$VERSION-$host/rustc cargo-$VERSION-$host/cargo \
	rust-std-$VERSION-$host/rust-std-$host \
	rust-std-$VERSION-x86_64-unknown-uefi/rust-std-x86_64-unknown-uefi; do
	tar -xf "$SRC/${c%/*}.tar.gz"
	(cd "$c" && rm -f manifest.in && cp -R . /out/)
done
cd /out
rm -rf bin/rust-gdb bin/rust-gdbgui bin/rust-lldb etc libexec lib/rustlib/etc share/man
cd /tmp

# pkg:llvm's clang.cfg links -static, and lld will not link a shared
# object into a static link, so the two shared libraries skip it.
sysroot=$(cd "$(cc -print-resource-dir)/../../../sysroot" && pwd)
ncc="clang --no-default-config --sysroot=$sysroot"

# musl's libc.so is its dynamic linker.
tar -xf "$SRC/musl-1.2.6.tar.gz"
cd musl-1.2.6
CC="$ncc" CFLAGS="-O2 -pipe" ./configure --prefix=/ --syslibdir=/lib --disable-static
make -j"$JOBS" lib/libc.so
mkdir -p /out/lib /out/etc
cp lib/libc.so /out/lib/ld-musl-x86_64.so.1
cd /tmp

# libunwind as libgcc_s.so.1: the vendor's binaries import _Unwind_*
# from it (musl's loader does not check symbol versions), and the libgcc
# helpers in $builtins (compiler-rt's, with default visibility). The frame
# APIs are upstream's no-op __register_frame_info and friends: in a
# non-PIE binary (rust-lld) gcc's crtbegin calls them through a PLT slot
# that must not be empty.
builtins=popcountdi2
llvm=llvm-project-23.1.3.src
tar -xf "$SRC/$llvm.tar.xz" $llvm/libunwind $llvm/compiler-rt/lib/builtins
cd $llvm/libunwind
for f in src/*.c src/*.cpp src/*.S; do
	case $f in
	*.cpp) fl="-std=c++17 -fno-exceptions -fno-rtti -nostdinc++ -fstrict-aliasing" ;;
	*.c) fl=-std=c99 ;;
	*) fl= ;;
	esac
	$ncc -O2 -fPIC -funwind-tables -DNDEBUG -D_LIBUNWIND_IS_NATIVE_ONLY \
		-D_LIBUNWIND_SUPPORT_FRAME_APIS -Iinclude $fl \
		-c "$f" -o "/tmp/unwind-${f##*/}.o"
done
for b in $builtins; do
	$ncc -O2 -fPIC -c ../compiler-rt/lib/builtins/$b.c -o /tmp/unwind-builtin-$b.o
done
# musl's libc.so has no soname: link it by -l so DT_NEEDED is libc.so,
# which the loader takes to mean itself, not this build path. Without
# --eh-frame-hdr libunwind cannot find its own unwind tables.
mkdir -p /tmp/libc && ln -s /out/lib/ld-musl-x86_64.so.1 /tmp/libc/libc.so
ld.lld -shared -soname libgcc_s.so.1 --eh-frame-hdr -o /out/lib/libgcc_s.so.1 \
	/tmp/unwind-*.o -L/tmp/libc -lc "$($ncc -print-libgcc-file-name)"
cd /out

# Every executable loads this pack's musl, which searches this pack's lib/
# (the loader reads <its prefix>/etc/ld-musl-x86_64.path).
for f in bin/* lib/rustlib/$host/bin/* lib/rustlib/$host/bin/gcc-ld/*; do
	[ -f "$f" ] && patchelf --print-interpreter "$f" >/dev/null 2>&1 || continue
	patchelf --set-interpreter "$LIVE/lib/ld-musl-x86_64.so.1" "$f"
done
echo "$LIVE/lib" >etc/ld-musl-x86_64.path
# Proc macros link against the shared libc and libgcc_s, found where
# rustc always looks: the target's lib/.
ln -s ../../../ld-musl-x86_64.so.1 lib/rustlib/$host/lib/libc.so
ln -s ../../../libgcc_s.so.1 lib/rustlib/$host/lib/libgcc_s.so

# Every symbol the vendor's binaries import must resolve here: fail now,
# not in some later build. A non-PIE executable's weak functions count
# too (see the frame APIs), except transactional memory's.
: >/tmp/needed
for f in bin/* lib/rustlib/$host/bin/* lib/rustlib/$host/bin/gcc-ld/* lib/*.so*; do
	[ -f "$f" ] || continue
	weak=
	llvm-readelf -h "$f" | grep -q 'Type:.*EXEC' && weak=WEAK
	llvm-readelf -W --dyn-syms "$f" |
		awk -v weak=$weak '$7 == "UND" && ($5 == "GLOBAL" || ($5 == weak && $4 == "FUNC")) { print $8 }' |
		grep -v '^_ITM_\|^_ZGTt' >>/tmp/needed || true
done
sed -i 's/@.*//' /tmp/needed
sort -u -o /tmp/needed /tmp/needed
llvm-readelf -W --dyn-syms lib/*.so* | awk '$5 != "LOCAL" && $7 != "UND" && $8 { print $8 }' |
	sed 's/@.*//' | sort -u >/tmp/defined
missing=$(comm -23 /tmp/needed /tmp/defined)
[ -z "$missing" ] || { echo "unresolved in pkg:rust: $missing" >&2; exit 1; }

# The pack is not live yet: run it through its loader, and check that
# rustc can still unwind (it reports errors by panicking).
LD_LIBRARY_PATH=/out/lib lib/ld-musl-x86_64.so.1 bin/rustc -vV
LD_LIBRARY_PATH=/out/lib lib/ld-musl-x86_64.so.1 bin/cargo -V
echo 'fn main() { x }' >/tmp/bad.rs
LD_LIBRARY_PATH=/out/lib lib/ld-musl-x86_64.so.1 bin/rustc --sysroot /out /tmp/bad.rs \
	-o /tmp/bad 2>/tmp/bad.err && exit 1
grep -q 'error\[E0425\]' /tmp/bad.err
if grep -q 'fatal runtime error' /tmp/bad.err; then exit 1; fi
