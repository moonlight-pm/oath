#!/bin/sh
# rustc host linker on Oath: rust-lld + pkg:glibc. Do not send the
# link line through Zig cc (Zig 0.16 rejects rustc's -fuse-ld=lld
# together with --dynamic-linker).
# rust-lld is a generic driver; Unix linking needs ld.lld.
lld=/oath/store/pkg/rustc/lib/rustlib/x86_64-unknown-linux-gnu/bin/gcc-ld/ld.lld
interp=/oath/store/pkg/glibc/lib/ld-linux-x86-64.so.2
glibc=/oath/store/pkg/glibc/lib
rlib=/oath/store/pkg/rustc/lib
n=0
for a in "$@"; do
	case "$a" in
	-fuse-ld=*|*gcc-ld*|-B*) continue ;;
	-Wl,*)
		a=${a#-Wl,}
		# rustc may pass comma-separated ld flags.
		IFS=,
		# shellcheck disable=SC2086
		for p in $a; do
			n=$((n + 1))
			eval "arg_$n=\$p"
		done
		unset IFS
		continue
		;;
	esac
	n=$((n + 1))
	eval "arg_$n=\$a"
done
i=1
set --
while [ "$i" -le "$n" ]; do
	eval "set -- \"\$@\" \"\$arg_$i\""
	i=$((i + 1))
done
# Skip rustc's lld-wrapper (argv0 games). Drive LLVM lld with -flavor gnu
# through pkg:glibc's loader. libz.so.1 lives in pkg:git (same as /bin/rustc).
zlib=/oath/store/pkg/git/lib
real_lld=/oath/store/pkg/rustc/lib/rustlib/x86_64-unknown-linux-gnu/bin/rust-lld
exec "$interp" --library-path "$rlib:$glibc:$zlib" "$real_lld" \
	-flavor gnu \
	--dynamic-linker="$interp" \
	-rpath "$glibc" \
	-L "$glibc" \
	"$@"
