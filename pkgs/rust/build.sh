# Output: bin/{rustc,rustdoc,cargo}, lib/ (librustc_driver, rustlib/) and
# share/doc/rust/. The vendor's binaries name /lib64/ld-linux-x86-64.so.2;
# Oath has no /lib64 (R17), so patchelf points them at /lib's loader.
host=x86_64-unknown-linux-gnu
for c in rustc-$VERSION-$host/rustc cargo-$VERSION-$host/cargo \
	rust-std-$VERSION-$host/rust-std-$host \
	rust-std-$VERSION-x86_64-unknown-uefi/rust-std-x86_64-unknown-uefi; do
	tar -xf "$SRC/${c%/*}.tar.xz"
	(cd "$c" && rm -f manifest.in && cp -R . /out/)
done
cd /out
rm -rf bin/rust-gdb bin/rust-gdbgui bin/rust-lldb etc libexec lib/rustlib/etc share/man
for f in bin/* lib/rustlib/$host/bin/* lib/rustlib/$host/bin/gcc-ld/*; do
	[ -f "$f" ] && patchelf --print-interpreter "$f" >/dev/null 2>&1 || continue
	patchelf --set-interpreter /lib/ld-linux-x86-64.so.2 "$f"
done

# It runs, and rustc can still unwind (it reports errors by panicking).
bin/rustc -vV
bin/cargo -V
echo 'fn main() { x }' >/tmp/bad.rs
bin/rustc --sysroot /out /tmp/bad.rs -o /tmp/bad 2>/tmp/bad.err && exit 1
grep -q 'error\[E0425\]' /tmp/bad.err
if grep -q 'fatal runtime error' /tmp/bad.err; then exit 1; fi
